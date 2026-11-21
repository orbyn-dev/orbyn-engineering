// Front door: the only module other features import from repos.
export { RepositoriesSection } from "./RepositoriesSection";
export { NewThreadDialog } from "./NewThreadDialog";
export { ReposChip } from "./ReposChip";
export { reposApi, setReposApi, useRepos, useThreadRepos } from "./api";
export { groupByRepo, plain } from "./model";
export type { ReposApi, RepoRow, ReposView, ThreadRepo } from "./contract";
