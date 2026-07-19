import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { Button } from "../ui/Button";

interface Caption {
  original: string;
  translated: string;
  start_ms: number;
  end_ms: number;
}

export const CaptionsView: React.FC = () => {
  const { t } = useTranslation();
  const [running, setRunning] = useState(false);
  const [captions, setCaptions] = useState<Caption[]>([]);
  const listEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unlisten = listen<Caption>("caption", (event) => {
      setCaptions((prev) => [...prev, event.payload]);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Auto-scroll to the newest caption
  useEffect(() => {
    listEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [captions]);

  const handleStart = async () => {
    const result = await commands.startCaptions();
    if (result.status === "ok") {
      setRunning(true);
    } else {
      toast.error(
        t("captions.startFailed", { defaultValue: "Failed to start captions" }),
        { description: result.error },
      );
    }
  };

  const handleStop = async () => {
    const result = await commands.stopCaptions();
    if (result.status === "ok") {
      setRunning(false);
    } else {
      toast.error(
        t("captions.stopFailed", { defaultValue: "Failed to stop captions" }),
        { description: result.error },
      );
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <Button onClick={handleStart} disabled={running}>
          {t("captions.start", { defaultValue: "Start" })}
        </Button>
        <Button variant="secondary" onClick={handleStop} disabled={!running}>
          {t("captions.stop", { defaultValue: "Stop" })}
        </Button>
        {running && (
          <span className="text-sm text-mid-gray">
            {t("captions.listening", { defaultValue: "Listening…" })}
          </span>
        )}
      </div>
      <div className="flex flex-col gap-3 border border-mid-gray/20 rounded-lg p-3 min-h-64 max-h-96 overflow-y-auto select-text cursor-text">
        {captions.length === 0 ? (
          <span className="text-sm text-mid-gray">
            {t("captions.empty", {
              defaultValue: "Captions will appear here.",
            })}
          </span>
        ) : (
          captions.map((caption, i) => (
            <div key={`${caption.start_ms}-${i}`} className="flex flex-col">
              <span className="text-base">{caption.translated}</span>
              <span className="text-xs text-mid-gray">{caption.original}</span>
            </div>
          ))
        )}
        <div ref={listEndRef} />
      </div>
    </div>
  );
};
