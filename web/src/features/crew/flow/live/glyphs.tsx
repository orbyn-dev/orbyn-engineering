import { Briefcase, Check, ChevronDown, ChevronRight, CircleCheck, Eye, GitBranch, GitCommitHorizontal, Search, ScanLine, Shield, TriangleAlert, User, Wrench, Workflow } from "lucide-react";
import type { ReactNode } from "react";
import type { FlowStage } from "../../../../generated/FlowStage";
import type { Glyph } from "./model";

const SIZE = 13;

/** The glyph of a cell. It carries no meaning alone: the cell's label names the state. */
export function CellGlyph({ glyph }: { glyph: Glyph }) {
  switch (glyph) {
    case "ok": return <CircleCheck size={SIZE} aria-hidden />;
    case "alert": return <TriangleAlert size={SIZE} aria-hidden />;
    case "eye": return <Eye size={SIZE} aria-hidden />;
    case "spin": return <i className="fv-spin" aria-hidden />;
    default: return null;
  }
}

const STAGE_ICON: Record<FlowStage, ReactNode> = {
  brief: <Briefcase size={SIZE} aria-hidden />,
  route: <Workflow size={SIZE} aria-hidden />,
  build: <Wrench size={SIZE} aria-hidden />,
  checks: <Check size={SIZE} aria-hidden />,
  code: <Eye size={SIZE} aria-hidden />,
  security: <Shield size={SIZE} aria-hidden />,
  finding: <ScanLine size={SIZE} aria-hidden />,
  merge: <GitBranch size={SIZE} aria-hidden />,
  main: <GitCommitHorizontal size={SIZE} aria-hidden />,
  you: <User size={SIZE} aria-hidden />,
};

export const StageIcon = ({ stage }: { stage: FlowStage }) => <>{STAGE_ICON[stage]}</>;
export const SearchIcon = () => <Search size={SIZE} aria-hidden />;
export const AlertIcon = () => <TriangleAlert size={SIZE} aria-hidden />;
export const DownIcon = () => <ChevronDown size={SIZE} aria-hidden />;
export const RightIcon = () => <ChevronRight size={SIZE} aria-hidden />;
