import { Search } from "lucide-react";
import type { Thread } from "../../../api/types";
import { navigate } from "../../../state/route";

/** The thread filter under the Projects label; Enter opens the highlighted hit. */
export function SearchField({
  query,
  highlight,
  results,
  setQuery,
  setHighlight,
  setSearching,
}: {
  query: string;
  highlight: number;
  results: Thread[];
  setQuery(query: string): void;
  setHighlight(update: number | ((at: number) => number)): void;
  setSearching(searching: boolean): void;
}) {
  return (
    <div className="search-field">
      <Search size={14} aria-hidden />
      <input
        type="search"
        aria-label="Search threads"
        placeholder="Filter threads"
        value={query}
        autoFocus
        onChange={(event) => {
          setQuery(event.target.value);
          setHighlight(0);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            setQuery("");
            setSearching(false);
          } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            const step = event.key === "ArrowDown" ? 1 : -1;
            setHighlight((at) => Math.min(results.length - 1, Math.max(0, at + step)));
          } else if (event.key === "Enter") {
            const hit = results[highlight];
            if (hit) navigate({ name: "thread", id: hit.id });
          }
        }}
      />
    </div>
  );
}
