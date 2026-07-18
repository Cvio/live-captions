import React from "react";

const BRAND_NAME = "VoiceTranslator";

const AppWordmark = ({
  width,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <span
      className={`font-bold tracking-tight text-logo-primary select-none ${className ?? ""}`}
      style={
        width ? { fontSize: Math.max(14, Math.round(width / 8)) } : undefined
      }
    >
      {BRAND_NAME}
    </span>
  );
};

export default AppWordmark;
