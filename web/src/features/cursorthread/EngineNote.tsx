import { Cpu } from "lucide-react";
import { DRow } from "../../product/ui/DetailRows";
import { useEngineNotice } from "./api";
import { noticeText } from "./NoticeCard";

/**
 * The Engine row of the thread's Details, for every engine: its name, and the model under it. An engine the service has a notice
 * for adds that notice instead, in small type, once it has been answered. A note, not a button.
 */
export function EngineNote({ engine, engineName, modelName }: { engine: string; engineName: string; modelName?: string | null }) {
  const notice = useEngineNotice(engine);
  const sub = notice?.acknowledged ? noticeText(engineName) : (modelName ?? null);
  return <DRow icon={<Cpu size={16} aria-hidden />} label="Engine" value={engineName} sub={sub} />;
}
