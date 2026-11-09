import { Eye, GitPullRequest, Sparkles, XCircle } from "lucide-react";

/** Starting points that only fill the message; sending is still the person's call. */
export const SUGGESTIONS = [
  { icon: XCircle, label: "Fix a failing test", text: "Find and fix a failing test." },
  { icon: GitPullRequest, label: "Review an open pull request", text: "Review the open pull request." },
  { icon: Eye, label: "Explain this codebase", text: "Explain how this codebase is organised." },
  { icon: Sparkles, label: "Use a skill", text: "$" },
] as const;
