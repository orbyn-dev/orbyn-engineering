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
  return [a, b, c, d];
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
