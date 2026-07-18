import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Input } from "../ui/Input";
import { useSettings } from "../../hooks/useSettings";

interface TtsEndpointProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const TtsEndpoint: React.FC<TtsEndpointProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const endpoint = getSetting("tts_endpoint") ?? "";
    const [localValue, setLocalValue] = useState(endpoint);

    // Sync with setting changes
    React.useEffect(() => {
      setLocalValue(endpoint);
    }, [endpoint]);

    return (
      <SettingContainer
        title={t("settings.postProcessing.output.ttsEndpoint.title")}
        description={t(
          "settings.postProcessing.output.ttsEndpoint.description",
        )}
        descriptionMode={descriptionMode}
        layout="horizontal"
        grouped={grouped}
      >
        <div className="flex items-center gap-2">
          <Input
            type="text"
            value={localValue}
            onChange={(event) => setLocalValue(event.target.value)}
            onBlur={() => updateSetting("tts_endpoint", localValue)}
            placeholder={t(
              "settings.postProcessing.output.ttsEndpoint.placeholder",
            )}
            variant="compact"
            disabled={isUpdating("tts_endpoint")}
            className="flex-1 min-w-[360px]"
          />
        </div>
      </SettingContainer>
    );
  },
);

TtsEndpoint.displayName = "TtsEndpoint";
