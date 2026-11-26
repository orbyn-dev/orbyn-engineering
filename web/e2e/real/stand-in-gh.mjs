#!/usr/bin/env node
/**
 * Answers the `gh` calls the service makes, from a JSON file, so forge routes run with
 * no network. ORBYN_GH_STATE names the file; ORBYN_GH_LOG collects each call.
 */
import { appendFileSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import process from "node:process";
import { acquireForgeLock } from "./forge-lock.mjs";

const args = process.argv.slice(2);
const statePath = process.env.ORBYN_GH_STATE;

// The service runs several of these at once, and each edits one file: without the
// lock a read sees a half-written file and two edits lose one. The service kills a
// call whose request went away, so a holder that has died no longer counts.
const release = acquireForgeLock(statePath);
process.on("exit", release);
// A cancelled request ends the stand-in with a signal, which skips the exit event.
for (const signal of ["SIGTERM", "SIGINT", "SIGHUP"]) {
  process.on(signal, () => {
    release();
    process.exit(1);
  });
}

const state = JSON.parse(readFileSync(statePath, "utf8"));
appendFileSync(process.env.ORBYN_GH_LOG, `${JSON.stringify(args)}\n`);
// A spec can make the forge slow, as a real one is: the app's reading state then stays observable for a moment.
// Off by default; `{ delayMs }` is set through the service's `ghEdit(0, …)` test helper.
if (typeof state.delayMs === "number" && state.delayMs > 0) await new Promise((done) => globalThis.setTimeout(done, state.delayMs));

const save = () => {
  // A rename, so the spec's own reads never catch it half done.
  writeFileSync(`${statePath}.${process.pid}`, JSON.stringify(state));
  renameSync(`${statePath}.${process.pid}`, statePath);
};
const flag = (name) => {
  const at = args.indexOf(name);
  return at < 0 ? undefined : args[at + 1];
};
const fail = (message) => {
  process.stderr.write(`${message}\n`);
  process.exit(1);
};
const out = (value) => process.stdout.write(typeof value === "string" ? `${value}\n` : `${JSON.stringify(value)}\n`);
const pull = (number) => state.pulls.find((entry) => entry.number === Number(number)) ?? fail(`GraphQL: Could not resolve to a PullRequest with the number of ${number}. (repository.pullRequest)`);

/** The fields only `gh pr view` shows, on top of a list row. */
const view = (entry) => ({
  comments: [],
  reviews: [],
  mergeable: "MERGEABLE",
  reviewRequests: (entry.reviewers ?? []).map((login) => ({ login })),
  headRefOid: "aaaaaaa",
  baseRefOid: "bbbbbbb",
  additions: 3,
  deletions: 1,
  changedFiles: 1,
  maintainerCanModify: false,
  autoMergeRequest: null,
  ...entry,
});

const threads = (number) => ({
  data: { repository: { pullRequest: { reviewThreads: { nodes: pull(number).threads ?? [] } } } },
});

const allThreads = () => state.pulls.flatMap((entry) => entry.threads ?? []);
/** The issues the stand-in holds, and one as `gh issue list` and `gh issue view --json` print it. */
const issues = () => state.issues ?? [];
const issue = (number) => issues().find((entry) => entry.number === Number(number)) ?? fail("GraphQL: Could not resolve to an Issue with the number of " + number + ". (repository.issue)");
/** The fields `gh issue list` and `gh issue view --json` know, as this fixture holds them. */
const ISSUE_FIELDS = ["number", "title", "url", "author", "state", "stateReason", "createdAt", "updatedAt", "closedAt", "assignees", "labels", "comments", "body", "closedByPullRequestsReferences"];
/** One issue as gh prints it: only the fields the call asked for. A name gh does not know fails the call, as real gh does. */
const issueView = (entry, requested) => {
  const fields = requested.length > 0 ? requested : ISSUE_FIELDS;
  const unknown = fields.find((name) => !ISSUE_FIELDS.includes(name));
  if (unknown) fail("Unknown JSON field: " + unknown);
  const out = {};
  const list = ["comments", "assignees", "labels", "closedByPullRequestsReferences"];
  for (const name of fields) out[name] = entry[name] === undefined ? (list.includes(name) ? [] : null) : entry[name];
  return out;
};
const commentById = (id) =>
  state.pulls.flatMap((entry) => [...(entry.issueComments ?? []), ...(entry.threads ?? []).flatMap((thread) => thread.comments.nodes)]).find((comment) => String(comment.id ?? comment.databaseId) === id);

/** One connection page as GitHub answers it: the nodes after the cursor and the pageInfo beside them. */
const connection = (name, nodes, body) => {
  // A connection may name further arguments (`writersOnly:true`), so only `first` and `after` are read.
  const call = new RegExp(`${name}\\(first:(\\d+)([^)]*)\\)`).exec(body);
  if (!call) return undefined;
  // The cursor is opaque to the service; this stand-in numbers its pages, so a fixture that
  // outgrows one page is read to the end instead of being reported as the whole list.
  const after = /after:"([^"]*)"/.exec(call[2]);
  const start = after ? Number(after[1]) : 0;
  const page = nodes.slice(start, start + Number(call[1]));
  const end = start + page.length;
  const more = end < nodes.length;
  return { pageInfo: { hasNextPage: more, endCursor: more ? String(end) : null }, nodes: page };
};

/** The reviews a fixture pull request holds: each review thread stands in for the review that left its comments. */
const reviewsOf = (entry) =>
  (entry.threads ?? []).map((thread) => ({
    state: "COMMENTED",
    author: thread.comments?.nodes?.[0]?.author ?? null,
    comments: { totalCount: thread.comments?.nodes?.length ?? 0 },
  }));

/** The fields of one pull request's batched GraphQL answer, before its review connections are paged. */
const detailsOf = (entry) => {
  const reviews = reviewsOf(entry);
  const latest = entry.latestReviews ?? reviews.filter((review) => review.author !== null).map(({ author, state }) => ({ author, state }));
  const requestedReviewers = (entry.reviewers ?? []).map((name) => ({
    requestedReviewer: name.startsWith("team/") ? { __typename: "Team", slug: name.slice(5) } : { __typename: "User", login: name },
  }));
  return {
    createdAt: entry.createdAt ?? "2026-09-01T00:00:00Z",
    // A draft has no merge state yet; every other fixture pull request merges cleanly.
    mergeable: entry.mergeable ?? (entry.isDraft ? "UNKNOWN" : "MERGEABLE"),
    comments: { totalCount: (entry.issueComments ?? []).length },
    reviews: {
      totalCount: reviews.length,
      pageInfo: { hasNextPage: false },
      nodes: reviews.map((review) => ({ comments: review.comments })),
    },
    // The opinionated reviews are paged at the asked limit, so a count above it is incomplete.
    approvals: latest.map(({ state }) => ({ state })),
    latest,
    requestedReviewers,
  };
};

/** REST and GraphQL through `gh api`. Field flags are `key=value` strings. */
function apiCall() {
  const field = (name) => {
    for (let at = 0; at < args.length - 1; at += 1) {
      if ((args[at] === "-f" || args[at] === "-F") && args[at + 1].startsWith(`${name}=`)) return args[at + 1].slice(name.length + 1);
    }
    return undefined;
  };
  if (args[1] === "user") return out({ login: state.viewer ?? "me" });
  const method = flag("-X") ?? "GET";
  const path = args.find((arg, at) => at > 0 && /^(repos|graphql|search\/|user\/starred\/)/.test(arg)) ?? "";
  if (path === "user/starred/orbyn-dev/orbyn-engineering") process.exit(state.starred ? 0 : 1);
  // The open and closed totals behind a listing (\`gh api -X GET search/issues --jq .total_count\`).
  if (path.startsWith("search/issues")) {
    const q = field("q") ?? "";
    const wanted = /is:(open|closed)\b/.exec(q)?.[1] ?? "";
    const total = (state.issues ?? []).filter((entry) => !wanted || (entry.state ?? "OPEN").toLowerCase() === wanted).length;
    return out(args.includes("--jq") ? String(total) : { total_count: total, incomplete_results: false, items: [] });
  }
  if (path === "graphql") {
    const query = field("query") ?? "";
    // GitHub's own viewed state: the files connection reads each changed file's
    // `viewerViewedState`, and the mark mutations set or clear one path. The fixture holds it as
    // `viewed: string[]`, the paths the viewer marked viewed.
    if (query.includes("ViewedFiles")) {
      if (state.rateLimited) return fail("GraphQL: API rate limit already exceeded");
      const entry = pull(field("number"));
      return out({
        data: {
          repository: {
            pullRequest: {
              id: `PR_${entry.number}`,
              files: {
                pageInfo: { hasNextPage: false, endCursor: null },
                nodes: (entry.files ?? []).map((file) => ({
                  path: file.filename,
                  viewerViewedState: (entry.viewed ?? []).includes(file.filename) ? "VIEWED" : "UNVIEWED",
                })),
              },
            },
          },
        },
      });
    }
    if (query.includes("ViewedId")) {
      if (state.rateLimited) return fail("GraphQL: API rate limit already exceeded");
      const entry = pull(field("number"));
      return out({ data: { repository: { pullRequest: { id: `PR_${entry.number}` } } } });
    }
    if (query.includes("MarkFileAsViewed") || query.includes("UnmarkFileAsViewed")) {
      if (state.rateLimited) return fail("GraphQL: API rate limit already exceeded");
      const entry = pull(String(field("id")).replace("PR_", ""));
      const path = field("path");
      const viewed = (entry.viewed ?? []).filter((name) => name !== path);
      if (query.includes("MarkFileAsViewed")) viewed.push(path);
      entry.viewed = viewed;
      save();
      return out({ data: { [query.includes("MarkFileAsViewed") ? "markFileAsViewed" : "unmarkFileAsViewed"]: { pullRequest: { id: `PR_${entry.number}` } } } });
    }
    if (query.includes("reviewRequests") || query.includes("latestReviews")) {
      // The batched details query and its review-connection follow-ups (#1544): one alias
      // `p<number>` per pull request, each connection carrying the pageInfo and counts GitHub
      // sends. A follow-up names only the connection it pages, reviewRequests or latestReviews.
      // The service reads a connection without pageInfo as an unknown one, so an answer
      // missing them leaves the row without a comment count, reviewers or merge state.
      if (state.rateLimited) return fail("GraphQL: API rate limit already exceeded");
      const aliases = [...query.matchAll(/p(\d+):pullRequest\(number:\d+\)\{/g)].map((match, at, all) => ({
        number: Number(match[1]),
        body: query.slice((match.index ?? 0) + match[0].length, all[at + 1]?.index),
      }));
      const repository = {};
      for (const { number, body } of aliases) {
        const entry = state.pulls.find((candidate) => candidate.number === number);
        if (!entry) continue;
        const { approvals, latest, requestedReviewers, ...answer } = detailsOf(entry);
        const requests = connection("reviewRequests", requestedReviewers, body);
        if (requests) answer.reviewRequests = requests;
        const reviews = connection("latestReviews", latest, body);
        if (reviews) answer.latestReviews = reviews;
        const opinionated = connection("latestOpinionatedReviews", approvals, body);
        if (opinionated) answer.latestOpinionatedReviews = { totalCount: approvals.length, ...opinionated };
        repository[`p${number}`] = answer;
      }
      return out({ data: { viewer: { login: state.viewer ?? "me" }, repository } });
    }
    if (query.includes("reviewThreads")) return out(threads(field("number")));
    const thread = allThreads().find((entry) => entry.id === field("id"));
    if (thread) {
      thread.isResolved = query.includes("resolveReviewThread") && !query.includes("unresolve");
      save();
    }
    return out({ data: {} });
  }
  const local = path.replace("repos/{owner}/{repo}/", "");
  let match;
  // The Issues client names the repository's own scope in the endpoint (`repos/o/r/…`) rather than the pull requests'
  // `{owner}/{repo}` placeholders, so the candidates read and the three edit writes (#1593) are answered here from the
  // same fixture: the assignable people, one PATCH of the issue's own fields or state, and one comment POST.
  const named = /^repos\/[^/]+\/[^/]+\/(.*)$/.exec(path)?.[1] ?? "";
  const [namedRoute] = named.split("?");
  if (namedRoute === "assignees") return out(state.people.map((login) => ({ login })));
  if ((match = /^issues\/(\d+)$/.exec(namedRoute)) && method === "PATCH") {
    const entry = issue(match[1]);
    const patch = JSON.parse(readFileSync(0, "utf8"));
    if (typeof patch.title === "string") entry.title = patch.title;
    if (typeof patch.body === "string") entry.body = patch.body;
    if (Array.isArray(patch.labels)) entry.labels = patch.labels.map((name) => ({ name }));
    if (Array.isArray(patch.assignees)) entry.assignees = patch.assignees.map((login) => ({ login }));
    if (patch.state === "open" || patch.state === "closed") {
      entry.state = patch.state.toUpperCase();
      entry.stateReason = patch.state === "closed" ? String(patch.state_reason ?? "").toUpperCase() : "";
      entry.closedAt = patch.state === "closed" ? new Date().toISOString() : "";
    }
    // GitHub moves `updatedAt` on every write, as the Issues list sorts on.
    entry.updatedAt = new Date().toISOString();
    save();
    return out({});
  }
  if ((match = /^issues\/(\d+)\/comments$/.exec(namedRoute)) && method === "POST") {
    const entry = issue(match[1]);
    const body = JSON.parse(readFileSync(0, "utf8")).body;
    const id = state.next++;
    const createdAt = new Date().toISOString();
    // Stored as `gh issue view` reads a comment back, answered with the REST shape the service parses.
    entry.comments = [...(entry.comments ?? []), { id: `IC_${id}`, author: { login: state.viewer ?? "me" }, body, createdAt }];
    save();
    return out({ id, node_id: `IC_${id}`, user: { login: state.viewer ?? "me" }, body, created_at: createdAt });
  }
  if (local.startsWith("assignees")) return out(state.people.map((login) => ({ login })));
  if ((match = /^issues\/(\d+)\/comments/.exec(local))) return out(pull(match[1]).issueComments ?? []);
  if ((match = /^issues\/(\d+)\/timeline/.exec(local))) return out(pull(match[1]).timeline ?? []);
  if ((match = /^pulls\/(\d+)\/files/.exec(local))) return out(pull(match[1]).files ?? []);
  if ((match = /^pulls\/(\d+)\/commits/.exec(local))) return out(pull(match[1]).commits ?? []);
  if ((match = /^commits\/(\w+)$/.exec(local))) {
    const commit = state.pulls.flatMap((entry) => entry.commits ?? []).find((entry) => entry.sha === match[1]);
    return out(commit ?? {});
  }
  if ((match = /^pulls\/(\d+)\/comments\/(\d+)\/replies/.exec(local))) {
    const thread = (pull(match[1]).threads ?? []).find((entry) => entry.comments.nodes.some((comment) => String(comment.databaseId) === match[2]));
    thread.comments.nodes.push({ databaseId: state.next++, body: field("body"), author: { login: "me" } });
    save();
    return out({});
  }
  if ((match = /^(issues|pulls)\/comments\/(\d+)\/reactions/.exec(local))) {
    const comment = commentById(match[2]);
    comment.reactions = [...(comment.reactions ?? []), field("content")];
    save();
    return out({});
  }
  if ((match = /^(issues|pulls)\/comments\/(\d+)$/.exec(local))) {
    commentById(match[2]).body = field("body");
    save();
    return out({});
  }
  if ((match = /^pulls\/(\d+)\/reviews/.exec(local))) {
    pull(match[1]).submitted = JSON.parse(readFileSync(0, "utf8"));
    save();
    return out({});
  }
  if ((match = /^pulls\/(\d+)$/.exec(local)) && method === "PATCH") {
    pull(match[1]).maintainerCanModify = field("maintainer_can_modify") === "true";
    save();
    return out({});
  }
  if (/^actions\/runs\/\d+\/approve/.test(local)) return out({});
  if (/^git\/refs\/heads\//.test(local) && method === "DELETE") return out({});
  return fail(`stand-in gh has no api answer for: ${args.join(" ")}`);
}

const route = `${args[0]} ${args[1]}`;
if (args[0] === "api") {
  apiCall();
  process.exit(0);
}
switch (route) {
  case "auth status":
    process.stderr.write("Logged in to github.com\n");
    break;
  case "repo create":
    state.repos.push(args[2]);
    save();
    out(`https://github.com/o/${args[2]}`);
    break;
  // The labels as the fixture holds them: a name, a real GitHub colour and a description, narrowed to the fields the
  // call asked for, as `gh label list --json` does (the Issues candidates read asks name,color; a pull request asks
  // name,description).
  case "label list": {
    const asked = (flag("--json") ?? "").split(",").map((name) => name.trim()).filter(Boolean);
    out(state.labels.map((entry) => {
      const label = typeof entry === "string" ? { name: entry } : entry;
      return asked.length === 0 ? label : Object.fromEntries(asked.filter((name) => name in label).map((name) => [name, label[name]]));
    }));
    break;
  }
  // The issue reads the service makes through `gh`: a listing with its four filters, and one issue as a detail.
  case "issue list": {
    const labels = args.reduce((all, arg, at) => (arg === "--label" && at + 1 < args.length ? [...all, args[at + 1]] : all), []);
    const wanted = flag("--state") ?? "open";
    const assignee = flag("--assignee");
    const search = (flag("--search") ?? "").trim();
    const words = search.split(/\s+/).filter((word) => word && !word.includes(":"));
    const unassigned = search.split(/\s+/).includes("no:assignee");
    const limit = Number(flag("--limit") ?? 30);
    const requested = (flag("--json") ?? "").split(",").map((name) => name.trim()).filter(Boolean);
    const rows = issues()
      .filter((entry) => wanted === "all" || (entry.state ?? "OPEN").toLowerCase() === wanted)
      .filter((entry) => labels.every((name) => (entry.labels ?? []).some((label) => label.name === name)))
      .filter((entry) => {
        if (!assignee) return true;
        const login = assignee === "@me" ? state.viewer ?? "me" : assignee;
        return (entry.assignees ?? []).some((who) => who.login === login);
      })
      .filter((entry) => !unassigned || (entry.assignees ?? []).length === 0)
      .filter((entry) => words.every((word) => (entry.title + " " + (entry.body ?? "")).toLowerCase().includes(word.toLowerCase())))
      .slice(0, Number.isFinite(limit) && limit > 0 ? limit : 30)
      .map((entry) => issueView(entry, requested));
    out(rows);
    break;
  }
  case "issue view": {
    const entry = issue(args[2]);
    const requested = (flag("--json") ?? "").split(",").map((name) => name.trim()).filter(Boolean);
    out(issueView(entry, requested));
    break;
  }
  case "pr list":
    out(state.pulls.filter((entry) => (flag("--state") === "all" ? true : entry.state.toLowerCase() === flag("--state"))));
    break;
  case "pr view":
    out(view(pull(args[2])));
    break;
  case "pr checks":
    out(pull(args[2]).checks ?? []);
    break;
  case "pr create": {
    const number = state.next++;
    state.pulls.push({
      ...state.template,
      number,
      title: flag("--title"),
      body: flag("--body"),
      headRefName: flag("--head"),
      baseRefName: flag("--base") ?? "main",
      url: `https://github.com/o/r/pull/${number}`,
    });
    save();
    out(`https://github.com/o/r/pull/${number}`);
    break;
  }
  case "pr edit": {
    const entry = pull(args[2]);
    if (flag("--title")) entry.title = flag("--title");
    if (flag("--body") !== undefined) entry.body = flag("--body");
    if (flag("--base")) entry.baseRefName = flag("--base");
    if (flag("--add-reviewer")) entry.reviewers = [...(entry.reviewers ?? []), ...flag("--add-reviewer").split(",")];
    if (flag("--remove-reviewer")) entry.reviewers = (entry.reviewers ?? []).filter((name) => !flag("--remove-reviewer").split(",").includes(name));
    if (flag("--add-label")) entry.labels = [...entry.labels, ...flag("--add-label").split(",").map((name) => ({ name }))];
    if (flag("--remove-label")) entry.labels = entry.labels.filter((label) => !flag("--remove-label").split(",").includes(label.name));
    save();
    break;
  }
  case "pr ready":
    pull(args[2]).isDraft = args.includes("--undo");
    save();
    break;
  case "pr close":
    pull(args[2]).state = "CLOSED";
    save();
    break;
  case "pr reopen":
    pull(args[2]).state = "OPEN";
    save();
    break;
  case "pr merge":
    // The expected head is checked as GitHub does: a branch that moved on since the page was read is refused.
    if (flag("--match-head-commit") && flag("--match-head-commit") !== (pull(args[2]).headNow ?? "aaaaaaa")) {
      fail("X Pull request is not mergeable: Head branch was modified. Review and try the merge again.");
    }
    if (args.includes("--auto")) pull(args[2]).autoMergeRequest = {};
    else pull(args[2]).state = "MERGED";
    pull(args[2]).mergeBody = flag("--body");
    save();
    break;
  case "pr comment": {
    const entry = pull(args[2]);
    entry.issueComments = [...(entry.issueComments ?? []), { id: state.next++, body: flag("--body"), user: { login: "me" } }];
    save();
    break;
  }
  case "pr review":
    pull(args[2]).reviewed = args.find((arg) => /^--(approve|request-changes|comment)$/.test(arg));
    pull(args[2]).reviewBody = flag("--body");
    save();
    break;
  case "pr revert":
  case "pr update-branch":
    break;
  default:
    fail(`stand-in gh has no answer for: ${args.join(" ")}`);
}
