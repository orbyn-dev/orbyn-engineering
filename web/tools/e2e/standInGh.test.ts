import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, it } from "vitest";

/**
 * The stand-in `gh` the real-service specs run: the service's own GraphQL reads, answered from a
 * fixture file. These tests start it as the service does, without a browser or a running service.
 */
const standIn = join(import.meta.dirname, "../../e2e/real/stand-in-gh.mjs");

interface Connection {
  totalCount?: number;
  pageInfo: { hasNextPage: boolean; endCursor?: string | null };
  nodes: Record<string, unknown>[];
}

interface PullAnswer {
  reviewRequests?: Connection;
  latestReviews?: Connection;
  latestOpinionatedReviews?: Connection;
}

interface Answer {
  data: { repository?: Record<string, PullAnswer> };
}

/** One `gh api graphql` call against a fixture state file, as the service makes it. */
function graphql(state: Record<string, unknown>, query: string): Answer {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-stand-in-gh-"));
  try {
    const statePath = join(dir, "state.json");
    const logPath = join(dir, "calls.jsonl");
    writeFileSync(statePath, JSON.stringify(state));
    writeFileSync(logPath, "");
    const call = spawnSync(process.execPath, [standIn, "api", "graphql", "-F", "owner=o", "-F", "repo=r", "-f", `query=${query}`], {
      encoding: "utf8",
      env: { ...process.env, ORBYN_GH_STATE: statePath, ORBYN_GH_LOG: logPath },
      timeout: 10_000,
    });
    expect(call.stderr).toBe("");
    expect(call.status).toBe(0);
    return JSON.parse(call.stdout) as Answer;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** The batched details query: both paged review connections, their counts, and the opinionated count. */
const detailsQuery = (numbers: number[]) =>
  `query($owner:String!,$repo:String!){viewer{login} repository(owner:$owner,name:$repo){${numbers
    .map(
      (number) =>
        `p${number}:pullRequest(number:${number}){createdAt mergeable ` +
        "reviewRequests(first:50){pageInfo{hasNextPage endCursor} nodes{requestedReviewer{__typename ...on User{login} ...on Mannequin{login} ...on Team{slug}}}} " +
        "latestReviews(first:50){pageInfo{hasNextPage endCursor} nodes{state author{login}}} " +
        "latestOpinionatedReviews(first:100,writersOnly:true){totalCount pageInfo{hasNextPage} nodes{state}} " +
        "comments{totalCount} reviews(first:100){totalCount pageInfo{hasNextPage} nodes{comments{totalCount}}}}",
    )
    .join(" ")}}}`;

/** One connection's next page, as the service asks when a first page names one. */
const nextPage = (connection: "reviewRequests" | "latestReviews", number: number, after: string, nodes: string) =>
  "query($owner:String!,$repo:String!){repository(owner:$owner,name:$repo){" +
  `p${number}:pullRequest(number:${number}){${connection}(first:50,after:"${after}"){pageInfo{hasNextPage endCursor} nodes{${nodes}}}}}}`;

it("pages the opinionated reviews at the asked limit and keeps a shorter connection whole", () => {
  const reviews = Array.from({ length: 101 }, (_, at) => ({ author: { login: `reviewer${at}` }, state: "APPROVED" }));
  const answer = graphql(
    {
      viewer: "me",
      pulls: [
        { number: 7, title: "Busy", latestReviews: reviews },
        { number: 8, title: "Quiet", latestReviews: reviews.slice(0, 3) },
      ],
    },
    detailsQuery([7, 8]),
  );
  // The service asks for 100 opinionated reviews: a count above that limit is incomplete.
  expect(answer.data.repository?.p7?.latestOpinionatedReviews).toMatchObject({ totalCount: 101, pageInfo: { hasNextPage: true } });
  expect(answer.data.repository?.p7?.latestOpinionatedReviews?.nodes).toHaveLength(100);
  expect(answer.data.repository?.p8?.latestOpinionatedReviews).toMatchObject({ totalCount: 3, pageInfo: { hasNextPage: false } });
  expect(answer.data.repository?.p8?.latestOpinionatedReviews?.nodes).toHaveLength(3);
});

it("answers a latest-reviews-only follow-up page and still answers a reviewer-only one", () => {
  const reviews = Array.from({ length: 51 }, (_, at) => ({ author: { login: at === 50 ? "me" : `reviewer${at}` }, state: "COMMENTED" }));
  const reviewers = Array.from({ length: 51 }, (_, at) => `reviewer${at}`);
  const state = { viewer: "me", pulls: [{ number: 7, title: "Busy", latestReviews: reviews, reviewers }] };

  const first = graphql(state, detailsQuery([7]));
  expect(first.data.repository?.p7?.latestReviews?.pageInfo).toMatchObject({ hasNextPage: true, endCursor: "50" });
  expect(first.data.repository?.p7?.reviewRequests?.pageInfo).toMatchObject({ hasNextPage: true, endCursor: "50" });

  // More than one page of reviews and no requested reviewers: the follow-up names only latestReviews.
  const lastReview = graphql(state, nextPage("latestReviews", 7, "50", "state author{login}")).data.repository?.p7?.latestReviews;
  expect(lastReview).toMatchObject({ pageInfo: { hasNextPage: false }, nodes: [{ author: { login: "me" }, state: "COMMENTED" }] });
  const lastReviewer = graphql(state, nextPage("reviewRequests", 7, "50", "requestedReviewer{__typename ...on User{login}}")).data.repository?.p7?.reviewRequests;
  expect(lastReviewer?.nodes).toHaveLength(1);
});
