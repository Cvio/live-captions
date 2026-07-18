import React from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown, type DropdownOption } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";

interface TtsLanguageProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const TtsLanguage: React.FC<TtsLanguageProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const currentLanguage = getSetting("tts_language") ?? "auto";

  const options: DropdownOption[] = [
    {
      value: "auto",
      label: t("settings.postProcessing.output.ttsLanguage.options.auto"),
    },
    {
      value: "es",
      label: t("settings.postProcessing.output.ttsLanguage.options.spanish"),
    },
    {
      value: "zh",
      label: t("settings.postProcessing.output.ttsLanguage.options.mandarin"),
    },
    {
      value: "en",
      label: t("settings.postProcessing.output.ttsLanguage.options.english"),
    },
  ];

  const handleSelect = async (value: string) => {
    if (value === currentLanguage) return;
    try {
      await updateSetting("tts_language", value);
    } catch (error) {
      console.error("Failed to update TTS language:", error);
    }
  };

  return (
    <SettingContainer
      title={t("settings.postProcessing.output.ttsLanguage.title")}
      description={t("settings.postProcessing.output.ttsLanguage.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="horizontal"
    >
      <Dropdown
        options={options}
        selectedValue={currentLanguage}
        onSelect={handleSelect}
        disabled={isUpdating("tts_language")}
      />
    </SettingContainer>
  );
};
