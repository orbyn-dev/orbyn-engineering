import { Cpu } from "lucide-react";
import { DRow } from "../../product/ui/DetailRows";
import { useEngineNotice } from "./api";
import { noticeText } from "./NoticeCard";

/**
 * The Engine row of the thread's Details: the engine's name with the notice in small type, once the notice has been answered.
 * A note, not a button. It is drawn only for an engine the service has a notice for, which is read, never named here.
 */
export function EngineNote({ engine, engineName }: { engine: string; engineName: string }) {
  const notice = useEngineNotice(engine);
  if (!notice?.acknowledged) return null;
  return <DRow icon={<Cpu size={16} aria-hidden />} label="Engine" value={engineName} sub={noticeText(engineName)} />;
}
