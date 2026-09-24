import { useQuery } from "@tanstack/react-query";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import { invokeTauri } from "@/shared/api/tauri";

export type ChannelFolder = { path: string | null; revision: number };
export type WorkingFolderSettings = {
  targetId: string;
  supported: boolean;
  defaultEnvKey: string;
  remote: boolean;
  folder: ChannelFolder | null;
};
export type WorkingFolderTarget = {
  id: string;
  agentPubkey: string;
  label: string;
};

export function useWorkingFolderScope() {
  const { activeCommunity } = useCommunities();
  const identity = useIdentityQuery();
  return {
    expectedRelayUrl: activeCommunity?.relayUrl ?? "",
    expectedSignerPubkey: identity.data?.pubkey ?? "",
  };
}

export function useWorkingFolderSettings(
  agentPubkey: string,
  channelId?: string,
  enabled = true,
) {
  const scope = useWorkingFolderScope();
  const query = useQuery({
    queryKey: [
      "working-folder",
      scope.expectedRelayUrl,
      scope.expectedSignerPubkey,
      agentPubkey,
      channelId ?? null,
    ],
    queryFn: () =>
      invokeTauri<WorkingFolderSettings>("get_working_folder_settings", {
        ...scope,
        agentPubkey,
        channelId: channelId ?? null,
      }),
    enabled:
      enabled &&
      Boolean(
        agentPubkey && scope.expectedRelayUrl && scope.expectedSignerPubkey,
      ),
    staleTime: 0,
    refetchOnWindowFocus: false,
    retry: false,
  });
  return { ...query, scope };
}
