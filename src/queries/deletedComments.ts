import { create } from "zustand";

/**
 * Session-only set of comment event ids the user has deleted.
 *
 * Relay indexing of kind:5 deletions lags, so a naive
 * invalidate-and-refetch can bring the comment back before the deletion
 * has propagated. We filter fetched results against this set for the
 * lifetime of the tab, and rely on the relay to eventually reflect the
 * deletion on the next cold start.
 */
type State = {
  ids: Set<string>;
  add: (id: string) => void;
};

export const useDeletedCommentIds = create<State>((set) => ({
  ids: new Set(),
  add: (id) =>
    set((s) => {
      if (s.ids.has(id)) return s;
      const next = new Set(s.ids);
      next.add(id);
      return { ids: next };
    }),
}));
