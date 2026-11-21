export type TokenKind = "command" | "flag" | "string" | "variable" | "operator" | "comment" | "plain" | "space";

export interface Token {
  text: string;
  kind: TokenKind;
}

const PIECE = /(\s+)|("(?:[^"\\]|\\.)*"?|'[^']*'?)|(\$\{[^}]*\}?|\$\([^)]*\)?|\$[A-Za-z_][A-Za-z0-9_]*|\$[?!#@*$0-9])|(\|\||&&|[|;<>&]+)|(#.*)|([^\s"'$|&;<>#]+|\S)/gy;

/** Splits a line into coloured pieces. The pieces join back to exactly the line, so a layer under the field lines up. */
export function tokenize(line: string): Token[] {
  const tokens: Token[] = [];
  let expectCommand = true;
  PIECE.lastIndex = 0;
  for (let match = PIECE.exec(line); match; match = PIECE.exec(line)) {
    const text = match[0];
    if (match[1] !== undefined) tokens.push({ text, kind: "space" });
    else if (match[2] !== undefined) {
      tokens.push({ text, kind: "string" });
      expectCommand = false;
    } else if (match[3] !== undefined) {
      tokens.push({ text, kind: "variable" });
      expectCommand = false;
    } else if (match[4] !== undefined) {
      tokens.push({ text, kind: "operator" });
      expectCommand = !/[<>]/.test(text);
    } else if (match[5] !== undefined) tokens.push({ text, kind: "comment" });
    else if (/^[A-Za-z_][A-Za-z0-9_]*=/.test(text) && expectCommand) tokens.push({ text, kind: "plain" });
    else if (expectCommand) {
      tokens.push({ text, kind: "command" });
      expectCommand = false;
    } else tokens.push({ text, kind: /^-{1,2}[A-Za-z0-9]/.test(text) ? "flag" : "plain" });
  }
  return tokens;
}
