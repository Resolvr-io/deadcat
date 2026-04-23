import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

/**
 * Is the current session signing via a NIP-46 remote signer?
 *
 * Used to decide whether to surface "approve in your signer app"
 * hints during async signing operations. Cached across the app so
 * every surface (comment post, zap, delete) reads the same state.
 */
export function useNip46Connected(): boolean {
  const { data } = useQuery({
    queryKey: ["nip46Status"],
    queryFn: () =>
      invoke<{ connected: boolean } | null>("get_nip46_status").catch(
        () => null,
      ),
    staleTime: 30_000,
  });
  return data?.connected === true;
}
