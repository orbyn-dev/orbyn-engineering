import { useState } from "react";
import { api } from "./api";
import { toFailure, useAsync } from "../../lib/useAsync";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { TIER_LABELS } from "./usageModel";

const RATES = [
  { key: "input", label: "Input" },
  { key: "output", label: "Output" },
  { key: "cache_read", label: "Cached input" },
  { key: "cache_write", label: "Cache write" },
] as const;

const PRICE_TIERS = ["standard", "fast", "priority", "batch", "flex", "other"] as const;

const sameTier = (tier: string | undefined, wanted: string) => (tier ?? "standard") === wanted;

/** A model's rates per million tokens for one tier, or another model it counts as. */
export function ModelPriceDialog({ model, onClose, onSaved }: { model: string; onClose(): void; onSaved(): void }) {
  const settings = useAsync("usage-settings", (signal) => api.settings(signal));
  const [tier, setTier] = useState<string>("standard");
  const current = settings.data?.model_prices?.find((price) => price.model === model && sameTier(price.tier, tier));
  const alias = settings.data?.model_aliases?.find((entry) => entry.model === model);
  const [rates, setRates] = useState<Record<string, string>>({});
  const [countsAs, setCountsAs] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shown = (key: (typeof RATES)[number]["key"]) => rates[key] ?? (current ? String(current[key]) : "");
  const save = async () => {
    if (!settings.data) return;
    setBusy(true);
    setError(null);
    const target = (countsAs ?? alias?.counts_as ?? "").trim();
    const priced = RATES.some((rate) => shown(rate.key) !== "");
    const number = (key: (typeof RATES)[number]["key"]) => Number(shown(key));
    try {
      await api.updateSettings((current) => ({
        ...current,
        model_prices: [
          ...(current.model_prices ?? []).filter((price) => price.model !== model || !sameTier(price.tier, tier)),
          ...(priced
            ? [{
                model,
                input: number("input"),
                output: number("output"),
                cache_read: number("cache_read"),
                cache_write: number("cache_write"),
                ...(tier === "standard" ? {} : { tier }),
              }]
            : []),
        ],
        model_aliases: [
          ...(current.model_aliases ?? []).filter((entry) => entry.model !== model),
          ...(target ? [{ model, counts_as: target }] : []),
        ],
      }));
      onSaved();
      onClose();
    } catch (failure) {
      setError(`Your changes are kept. ${toFailure(failure).message}`);
      setBusy(false);
    }
  };
  return (
    <Dialog
      title={`Price for ${model}`}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button tone="primary" busy={busy} disabled={!settings.data} onClick={() => void save()}>
            Save price
          </Button>
        </>
      }
    >
      <p>Dollars per million tokens. Another tier never borrows the standard rate.</p>
      <Field label="Tier">
        {(props) => (
          <select
            {...props}
            value={tier}
            onChange={(event) => {
              setTier(event.target.value);
              setRates({});
            }}
          >
            {PRICE_TIERS.map((entry) => (
              <option key={entry} value={entry}>
                {TIER_LABELS[entry]}
              </option>
            ))}
          </select>
        )}
      </Field>
      {RATES.map((rate) => (
        <Field key={rate.key} label={rate.label}>
          {(props) => (
            <input
              {...props}
              type="number"
              min="0"
              step="any"
              value={shown(rate.key)}
              onChange={(event) => setRates({ ...rates, [rate.key]: event.target.value })}
            />
          )}
        </Field>
      ))}
      <Field label="Counts as" hint="Another model whose price this one follows.">
        {(props) => <input {...props} value={countsAs ?? alias?.counts_as ?? ""} onChange={(event) => setCountsAs(event.target.value)} />}
      </Field>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}
