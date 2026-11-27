import { createServer } from "node:http";
function f(el: HTMLElement, html: string, code: string) {
  // ruleid: orbyn-ts-eval
  eval(code);
  // ok: orbyn-ts-eval
  JSON.parse(code);
  // ruleid: orbyn-ts-inner-html
  el.innerHTML = html;
  // ok: orbyn-ts-inner-html
  el.innerHTML = "";
  // ruleid: orbyn-ts-storage-secret
  localStorage.setItem("authToken", html);
  // ok: orbyn-ts-storage-secret
  localStorage.setItem("theme", html);
}

createServer((request, response) => {
  // ruleid: orbyn-js-reflected-html-from-url
  response.end(`<p>${request.url}</p>`);
});
createServer((req, res) => {
  // ok: orbyn-js-reflected-html-from-url
  res.end(`<p>${encodeURIComponent(req.url)}</p>`);
  // ok: orbyn-js-reflected-html-from-url
  res.end("<p>static</p>");
});
export function escape(selector: string) {
  // ruleid: orbyn-js-regex-escape-skips-backslash
  const a = selector.replace(/[.[\]]/g, "\\$&");
  // ok: orbyn-js-regex-escape-skips-backslash
  const b = selector.replace(/[.[\\\]]/g, "\\$&");
  // ok: orbyn-js-regex-escape-skips-backslash
  const c = selector.replace(/[.[\]]/g, "_");
  // ok: orbyn-js-regex-escape-skips-backslash
  const d = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // ruleid: orbyn-js-regex-escape-skips-backslash
  const e = selector.replace(/[.[\]]/g, '\\$&');
  // ok: orbyn-js-regex-escape-skips-backslash
  const f = selector.replace(/[.[\\\]]/g, '\\$&');
  // A negated class matches a backslash, so prefixing one escapes it: no finding here.
  // ok: orbyn-js-regex-escape-skips-backslash
  const g = selector.replace(/[^a-zA-Z0-9_]/g, "\\$&");
  // ok: orbyn-js-regex-escape-skips-backslash
  const h = selector.replace(/[^a-zA-Z0-9_]/g, '\\$&');
  // The \W and \D shorthands match a backslash too, so they escape it as well.
  // ok: orbyn-js-regex-escape-skips-backslash
  const i = selector.replace(/[\W]/g, '\\$&');
  // ok: orbyn-js-regex-escape-skips-backslash
  const j = selector.replace(/[\D]/g, "\\$&");
  // \w does not match a backslash, so this class still loses one: a finding.
  // ruleid: orbyn-js-regex-escape-skips-backslash
  const k = selector.replace(/[\w.]/g, "\\$&");
  return [a, b, c, d, e, f, g, h, i, j, k];
}

export function names(name: string, text: string) {
  // ruleid: orbyn-js-replace-string-literal-first-only
  const a = name.replace("@", "%40");
  // ok: orbyn-js-replace-string-literal-first-only
  const b = name.replace(/@/g, "%40");
  // ruleid: orbyn-js-strip-html-comment-incomplete
  const c = text.replace(/<!--[\s\S]*?-->/g, "");
  // ok: orbyn-js-strip-html-comment-incomplete
  const d = text.replace(/\s+/g, " ");
  return [a, b, c, d];
}
