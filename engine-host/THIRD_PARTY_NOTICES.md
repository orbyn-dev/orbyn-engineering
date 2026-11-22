# Third-party notices for the engine host

The engine host depends on two proprietary SDKs. They are used as published, unmodified, as dependencies; none of their code is
copied into this repository. Their licence files are reproduced here as they ship in the packages; the app bundle carries this file in
`Contents/Resources/sdk` beside the SDKs (docs/sdk-default-plan.md, step 4). The permissions Orbyn Engineering relies on are not reproduced here.

## @anthropic-ai/claude-agent-sdk 0.3.321

Licence field: "SEE LICENSE IN README.md". `LICENSE.md` of the package:

> © Anthropic PBC. All rights reserved. Use is subject to the Legal Agreements outlined here: https://code.claude.com/docs/en/legal-and-compliance.

The package's README points to the Commercial Terms of Service (https://www.anthropic.com/legal/commercial-terms) and the Privacy
Policy (https://www.anthropic.com/legal/privacy). Orbyn does not ship the SDK's platform packages
(`@anthropic-ai/claude-agent-sdk-<platform>`, which hold Anthropic's own `claude` binary): the SDK runs the person's own installed
`claude` through `pathToClaudeCodeExecutable`, and the production install omits optional dependencies.

## @cursor/sdk 1.4.2

Licence field: "SEE LICENSE IN LICENSE.md". `LICENSE.md` of the package:

> # Cursor SDK License
>
> © Anysphere Inc. All rights reserved. Use is subject to Cursor's [Terms of Service](https://cursor.com/terms-of-service).

Its dependencies are `@bufbuild/protobuf` 1.10.0 (Apache-2.0 AND BSD-3-Clause), `@statsig/js-client` 3.31.0, `@connectrpc/connect` and
`@connectrpc/connect-web` (Apache-2.0), plus the platform packages `@cursor/sdk-<platform>` it ships native code in.

## Other npm dependencies

The remaining packages in `package-lock.json` are MIT, ISC, Apache-2.0, BSD-2-Clause, BSD-3-Clause or Unlicense, and keep their
own licence files in their packages. A generated notice for the shipped set is part of the bundle step.
