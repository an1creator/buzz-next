import { ConnectionsSettingsPanel } from "@/features/connections/ConnectionsSettingsPanel";
import { Button } from "@/shared/ui/button";
import { OnboardingFooter } from "./OnboardingFooter";
import { ONBOARDING_PRIMARY_CTA_CLASS } from "./OnboardingChrome";

/** Optional machine setup uses the same explicit connections as Settings. */
export function ConnectionsOnboardingStep({
  onContinue,
}: {
  onContinue: () => void;
}) {
  return (
    <div className="space-y-6" data-testid="onboarding-connections">
      <div>
        <h1 className="text-title font-normal">Where will your agents run?</h1>
        <p className="mt-2 text-base text-muted-foreground">
          Add this device or an SSH server. You can continue to conversations
          now and set up agents later in Settings → Agents → Connections.
        </p>
      </div>
      <ConnectionsSettingsPanel />
      <OnboardingFooter>
        <Button
          type="button"
          className={ONBOARDING_PRIMARY_CTA_CLASS}
          onClick={onContinue}
        >
          Continue to Buzz
        </Button>
      </OnboardingFooter>
    </div>
  );
}
