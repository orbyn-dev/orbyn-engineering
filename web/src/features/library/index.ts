// Front door: the only module other features import from library.
export { LibraryPanel, type TypeIntoTerminal } from "./LibraryPanel";
export { libraryApi, setLibraryApi, useLibraryOffered } from "./api";
export type { LibraryApi } from "./contract";
