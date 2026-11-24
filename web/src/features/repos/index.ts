// Front door: the only module other features import from repos.
export { RepositoriesSection } from "./RepositoriesSection";
export { NewThreadDialog } from "./NewThreadDialog";
export { ReposChip } from "./ReposChip";
export { RepoGitMenu } from "./RepoGitMenu";
export { reposApi, setReposApi, useRepos, useThreadRepos } from "./api";
export { checkCloneSource, groupByRepo, plain } from "./model";
export type { ReposApi, RepoRow, ReposView, ThreadRepo } from "./contract";
export { chooseRepo, RepoChooser, repoLabel, usePickedRepo, useRepoChoice, type RepoChoice } from "./choice";
export { TrustRepo } from "./TrustRepo";
export { TrustOffer } from "./TrustOffer";
export { canPickFolder, pickFolder } from "./folder";
