# @fishword/cli

JavaScript wrapper for the Fishword Rust CLI.

```js
import { fishwordPath } from "@fishword/cli";
```

The wrapper resolves binaries in this order:

```text
FISHWORD_CLI_PATH
target/debug/fishword
@fishword/cli-<platform>/bin/fishword
```

It also exposes a `fishword` npm binary.

The published package includes bundled Qwerty Learner dictionary assets. The
wrapper passes that asset directory to the Rust CLI so `fishword init` can create
the local database and seed the default decks without a separate import step.
