import { useCallback } from "react";
import { useStore } from "../../store";

export function SearchBar() {
  const search = useStore((s) => s.search);
  const searchOpen = useStore((s) => s.searchOpen);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      useStore.setState({ search: e.target.value });
    },
    [],
  );

  const openSearch = useCallback(() => {
    useStore.setState({ searchOpen: true });
  }, []);

  const closeSearch = useCallback(() => {
    useStore.setState({ searchOpen: false });
  }, []);

  return (
    <>
      {/* Desktop inline search */}
      <input
        value={search}
        onChange={handleChange}
        className="hidden h-9 w-48 min-w-[160px] max-w-[320px] flex-1 rounded-full border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-300 transition focus:ring-2 lg:block"
        placeholder="Search"
      />

      {/* Mobile search button */}
      <button
        onClick={openSearch}
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 lg:hidden"
      >
        <svg
          className="h-[18px] w-[18px]"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
      </button>

      {/* Mobile search overlay */}
      {searchOpen && (
        <div className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm lg:hidden">
          <div className="flex items-center gap-3 border-b border-slate-800 bg-slate-950 px-4 py-3">
            <input
              value={search}
              onChange={handleChange}
              className="h-10 flex-1 rounded-full border border-slate-700 bg-slate-900 px-4 text-sm text-slate-200 outline-none ring-emerald-300 transition focus:ring-2"
              placeholder="Search"
              autoFocus
            />
            <button
              onClick={closeSearch}
              className="shrink-0 text-sm text-slate-400 hover:text-slate-200"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </>
  );
}
