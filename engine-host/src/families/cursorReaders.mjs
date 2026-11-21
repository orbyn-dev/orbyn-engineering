// Read-only Cursor account, model-catalog and usage-limit readers for the `models` and `usage` requests.
// Same public HTTP routes as T3 Code's packages/provider-cursor (MIT, commit 9168ca7, test/fixtures/NOTICE):
// GET /v1/me and /v1/models with the API key as a bearer token, and the dashboard's GetCurrentPeriodUsage
// with the access token the key exchanges for. The key arrives only as CURSOR_API_KEY at spawn; nothing
// here returns or logs it, and every failure carries a fixed message, never a response body. Nothing here
// has run against the real Cursor service: the tests use a fake `fetch`.

const CATALOG_URL = "https://api.cursor.com";
const API_ENDPOINT = "https://api2.cursor.sh";
const TIMEOUT_MS = 10_000;
export const CATALOG_TTL_MS = 30 * 60_000;

const WINDOWS = [
  { id: "totalPercentUsed", label: "Overall" },
  { id: "autoPercentUsed", label: "Cursor Models" },
  { id: "apiPercentUsed", label: "Other Models" },
];

const isObject = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
const str = (v) => (typeof v === "string" && v !== "" ? v : undefined);
const clampPercent = (n) => Math.min(100, Math.max(0, n));

/** A reader failure the family maps to a protocol error `kind`; the message is fixed text. */
export class ReaderError extends Error {
  constructor(message, kind, extra = {}) {
    super(message);
    this.name = "CursorReaderError";
    this.kind = kind;
    Object.assign(this, extra);
  }
}

const kindOfStatus = (status) => (status === 401 ? "auth" : status === 403 ? "auth" : status === 429 ? "rate_limit" : status >= 500 ? "network" : "engine");

async function getJson(fetchImpl, url, init, label) {
  let response;
  try {
    response = await fetchImpl(url, { ...init, signal: AbortSignal.timeout(TIMEOUT_MS) });
  } catch {
    throw new ReaderError(`Cursor ${label} could not be reached.`, "network");
  }
  if (!response.ok) {
    let code;
    try {
      const body = await response.json();
      code = str(body?.error?.code) ?? str(body?.code);
    } catch {
      // no readable body
    }
    throw new ReaderError(`Cursor ${label} returned HTTP ${response.status}.`, kindOfStatus(response.status), { status: response.status, ...(code ? { code } : {}) });
  }
  try {
    return await response.json();
  } catch {
    throw new ReaderError(`Cursor ${label} returned an unreadable answer.`, "engine");
  }
}

function modelOf(item) {
  if (!isObject(item) || !str(item.id)) return undefined;
  const parameters = Array.isArray(item.parameters)
    ? item.parameters.filter((p) => isObject(p) && str(p.id)).map((p) => ({
        id: p.id,
        ...(str(p.displayName) ? { displayName: p.displayName } : {}),
        values: (Array.isArray(p.values) ? p.values : []).filter((v) => isObject(v) && str(v.value)).map((v) => ({ value: v.value, ...(str(v.displayName) ? { displayName: v.displayName } : {}) })),
      }))
    : undefined;
  const variants = Array.isArray(item.variants)
    ? item.variants.filter((v) => isObject(v)).map((v) => ({
        params: (Array.isArray(v.params) ? v.params : []).filter((p) => isObject(p) && str(p.id) && typeof p.value === "string").map((p) => ({ id: p.id, value: p.value })),
        displayName: str(v.displayName) ?? "",
        ...(str(v.description) ? { description: v.description } : {}),
        ...(typeof v.isDefault === "boolean" ? { isDefault: v.isDefault } : {}),
        ...(typeof v.isMaxMode === "boolean" ? { isMaxMode: v.isMaxMode } : {}),
      }))
    : undefined;
  return {
    id: item.id,
    displayName: str(item.displayName) ?? item.id,
    ...(str(item.description) ? { description: item.description } : {}),
    ...(Array.isArray(item.aliases) ? { aliases: item.aliases.filter((a) => typeof a === "string") } : {}),
    ...(parameters ? { parameters } : {}),
    ...(variants ? { variants } : {}),
  };
}

/** `{models, account?}`: the account label is left out when the team blocks it (HTTP 403 not plan_required), as T3 does. */
export function createCatalogReader({ fetch: fetchImpl = globalThis.fetch, env = process.env, apiKey, now = Date.now } = {}) {
  const baseUrl = (str(env.CURSOR_BACKEND_URL) ?? CATALOG_URL).replace(/\/$/, "");
  let cached;
  const headers = () => ({ authorization: `Bearer ${apiKey}`, accept: "application/json" });
  return async function readCatalog() {
    if (!apiKey) throw new ReaderError("Set CURSOR_API_KEY when the host starts to read the Cursor catalog.", "auth");
    if (cached && cached.until > now()) return cached.value;
    const [me, list] = await Promise.all([
      getJson(fetchImpl, `${baseUrl}/v1/me`, { headers: headers() }, "account").catch((error) => {
        if (error.status === 403 && error.code !== "plan_required") return undefined;
        throw error;
      }),
      getJson(fetchImpl, `${baseUrl}/v1/models`, { headers: headers() }, "model list"),
    ]);
    const models = (Array.isArray(list?.items) ? list.items : []).map(modelOf).filter(Boolean);
    const account = isObject(me)
      ? {
          ...(str(me.apiKeyName) ? { apiKeyName: me.apiKeyName } : {}),
          ...(str(me.createdAt) ? { createdAt: me.createdAt } : {}),
          ...(typeof me.userId === "number" ? { userId: me.userId } : {}),
          ...(str(me.userEmail) ? { email: me.userEmail } : {}),
          ...(str(me.userFirstName) ? { firstName: me.userFirstName } : {}),
          ...(str(me.userLastName) ? { lastName: me.userLastName } : {}),
        }
      : undefined;
    const value = { models, ...(account ? { account } : {}) };
    if (models.length > 0) cached = { value, until: now() + CATALOG_TTL_MS };
    return value;
  };
}

/** Plan percentages and the billing-cycle reset as limit windows (T3 usageLimits.ts). */
export function cursorUsageToLimits(response, checkedAt) {
  const end = Number(response?.billingCycleEnd);
  const start = Number(response?.billingCycleStart);
  const resetsAt = end > 0 && Number.isFinite(end) && end <= 8.64e15 ? new Date(end).toISOString() : undefined;
  const windowDurationMins = resetsAt && start > 0 && end > start ? Math.round((end - start) / 60_000) : 0;
  const windows = [];
  const plan = response?.planUsage;
  if (isObject(plan)) {
    for (const { id, label } of WINDOWS) {
      const used = plan[id];
      if (typeof used !== "number" || !Number.isFinite(used)) continue;
      windows.push({ id, kind: "monthly", label, usedPercent: clampPercent(used), ...(resetsAt ? { resetsAt } : {}), ...(windowDurationMins > 0 ? { windowDurationMins } : {}) });
    }
  }
  return windows.length > 0 ? { checkedAt, windows } : { checkedAt, windows: [], unavailable: "unsupported" };
}

export function createLimitsReader({ fetch: fetchImpl = globalThis.fetch, env = process.env, apiKey, now = Date.now } = {}) {
  return async function readLimits() {
    const checkedAt = new Date(now()).toISOString();
    try {
      const endpoint = (str(env.CURSOR_API_ENDPOINT) ?? API_ENDPOINT).replace(/\/$/, "");
      let token = str(env.CURSOR_AUTH_TOKEN?.trim());
      if (!token && apiKey) {
        if (endpoint !== API_ENDPOINT) return { checkedAt, windows: [], unavailable: "unsupported", message: "Cursor usage for an API key requires the default Cursor endpoint." };
        const exchanged = await getJson(fetchImpl, `${API_ENDPOINT}/auth/exchange_user_api_key`, { method: "POST", headers: { authorization: `Bearer ${apiKey}`, "content-type": "application/json" }, body: "{}" }, "key exchange");
        token = str(exchanged?.accessToken);
      }
      if (!token) return { checkedAt, windows: [], unavailable: "unsupported" };
      const body = await getJson(
        fetchImpl,
        `${endpoint}/aiserver.v1.DashboardService/GetCurrentPeriodUsage`,
        { method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json", "connect-protocol-version": "1", "x-cursor-client-type": "cli" }, body: "{}" },
        "usage",
      );
      return cursorUsageToLimits(body, checkedAt);
    } catch {
      return { checkedAt, windows: [], unavailable: "probeFailed", message: "Cursor could not read usage limits." };
    }
  };
}
