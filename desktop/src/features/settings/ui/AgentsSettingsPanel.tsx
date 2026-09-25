import { ConnectionsSettingsPanel } from "@/features/connections/ConnectionsSettingsPanel";
import {
  setKeepMentionedAgentsPinned,
  useKeepMentionedAgentsPinned,
} from "@/features/messages/lib/autoPinMentionedAgentsPreference";
import { Switch } from "@/shared/ui/switch";
import {
  SettingsOptionGroup,
  SettingsOptionGroupList,
  SettingsOptionRow,
} from "./SettingsOptionGroup";
import { SettingsSectionHeader } from "./SettingsSectionHeader";

export function AgentsSettingsPanel() {
  const automaticallyMentionAgents = useKeepMentionedAgentsPinned();

  return (
    <section className="min-w-0" data-testid="settings-agents">
      <SettingsSectionHeader
        title="Agents"
        description="Control agent conversations and connections to execution devices."
      />

      <SettingsOptionGroupList>
        <SettingsOptionGroup title="Conversations">
          <SettingsOptionRow data-testid="settings-automatic-agent-mentions">
            <div className="min-w-0">
              <label
                className="font-medium text-foreground"
                htmlFor="settings-automatic-agent-mentions-switch"
              >
                Automatically mention agents
              </label>
              <p
                className="mt-0.5 text-sm text-muted-foreground/70"
                data-settings-subcopy
              >
                Address selected agents in thread replies
              </p>
            </div>
            <Switch
              aria-label="Automatically mention agents"
              checked={automaticallyMentionAgents}
              id="settings-automatic-agent-mentions-switch"
              onCheckedChange={setKeepMentionedAgentsPinned}
            />
          </SettingsOptionRow>
        </SettingsOptionGroup>
        <ConnectionsSettingsPanel />
      </SettingsOptionGroupList>
    </section>
  );
}
