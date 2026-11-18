import { useAsync } from "../../../../lib/useAsync";
import { capacityApi } from "./api";

/** One project's capacity state. A read, never a write; a retry reads again. */
export const useCapacityState = (project: string) => useAsync(`crew-capacity:${project}`, (signal) => capacityApi.state(project, signal));

/** One project's review coverage since provenance began. */
export const useCoverage = (project: string) => useAsync(`crew-coverage:${project}`, (signal) => capacityApi.coverage(project, signal));
