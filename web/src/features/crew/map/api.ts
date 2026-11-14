// The Crew map's three reads: the tree of the crew, the labels of its messages, and the labels of its board facts. Reads only. None
// of them carries a message's text or a fact's value: a view that shows either has invented it. Every Crew route answers 404 while
// Crew Preview is off.
import { request } from "../../../api/client";
import type { FlowFactList } from "../../../generated/FlowFactList";
import type { FlowMap } from "../../../generated/FlowMap";
import type { FlowMessageList } from "../../../generated/FlowMessageList";

export const MESSAGE_PAGE = 50;

const get = <T,>(path: string, signal?: AbortSignal) => request<T>("GET", path, undefined, signal);

export function messagesPath(crew: string, member: string | null, offset: number): string {
  const params = new URLSearchParams({ crew, limit: String(MESSAGE_PAGE) });
  if (member) params.set("member", member);
  if (offset > 0) params.set("offset", String(offset));
  return `/api/crew/flow/messages?${params.toString()}`;
}

export const mapApi = {
  map: (crew: string, signal?: AbortSignal): Promise<FlowMap> => get<FlowMap>(`/api/crew/flow/map?crew=${encodeURIComponent(crew)}`, signal),
  messages: (crew: string, member: string | null, offset: number, signal?: AbortSignal): Promise<FlowMessageList> => get<FlowMessageList>(messagesPath(crew, member, offset), signal),
  facts: (crew: string, signal?: AbortSignal): Promise<FlowFactList> => get<FlowFactList>(`/api/crew/flow/facts?crew=${encodeURIComponent(crew)}`, signal),
};
