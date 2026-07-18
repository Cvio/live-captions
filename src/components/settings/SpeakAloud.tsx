import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface SpeakAloudProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SpeakAloud: React.FC<SpeakAloudProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("speak_aloud") ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("speak_aloud", enabled)}
        isUpdating={isUpdating("speak_aloud")}
        label={t("settings.postProcessing.output.speakAloud.label")}
        description={t("settings.postProcessing.output.speakAloud.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
