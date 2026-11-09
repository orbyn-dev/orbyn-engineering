import { GitCommitHorizontal, GitPullRequest, Upload } from "lucide-react";

export interface Plan {
  title: string;
  commit: boolean;
  push: boolean;
  pull: boolean;
}

export const PLANS: { label: string; plan: Plan; icon: typeof Upload }[] = [
  { label: "Commit", plan: { title: "Commit", commit: true, push: false, pull: false }, icon: GitCommitHorizontal },
  { label: "Commit and push", plan: { title: "Commit and push", commit: true, push: true, pull: false }, icon: Upload },
  {
    label: "Commit, push and create pull request",
    plan: { title: "Commit, push and create pull request", commit: true, push: true, pull: true },
    icon: GitPullRequest,
  },
  { label: "Push", plan: { title: "Push", commit: false, push: true, pull: false }, icon: Upload },
  {
    label: "Create pull request",
    plan: { title: "Create pull request", commit: false, push: true, pull: true },
    icon: GitPullRequest,
  },
];

/** The menu's own "Create pull request" entry, for other places that offer it. */
export const createPullRequestPlan = (): Plan => PLANS.find((entry) => entry.label === "Create pull request")!.plan;
