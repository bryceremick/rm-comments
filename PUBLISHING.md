# Publishing

Everything publishes automatically from GitHub Actions. **A push to `main` is a
release.** Day-to-day work happens on `dev`.

```
dev  ── push ──────────────► ci.yml: cargo test
  │
  └── merge/PR to main ────► publish.yml:
                               1. version check (Cargo.toml == plugin.json)
                               2. test + build binaries (5 platforms)
                               3. GitHub Release + binaries   ┐ each job checks its
                               4. crates.io publish           │ own published state —
                               5. Homebrew formula push       ┘ idempotent, re-runnable
```

- **Version unchanged?** Steps 3–5 detect their surface is already published and
  skip. A push to `main` without a bump is just a 5-platform test run.
- **Partial failure?** Re-run the workflow — completed surfaces skip themselves,
  the failed one retries.
- Plugin marketplace, Zed, binstall, and docs.rs need no publishing at all
  (they read from the repo / release / crates.io automatically).

## Releasing a new version

1. On `dev`: bump the version in **`Cargo.toml`** and **`.claude-plugin/plugin.json`**
   (CI fails the publish if they differ), commit, push.
2. Merge `dev` into `main` (PR or `git checkout main && git merge dev && git push`).
3. Done. Watch it if you like: `gh run watch`.

## One-time setup (the only manual steps — both are secrets only you can mint)

### 1. `CARGO_REGISTRY_TOKEN` — lets CI publish to crates.io

1. <https://crates.io> → log in with GitHub → **Account Settings → API Tokens →
   New Token**. Scopes: `publish-new` + `publish-update`.
2. Add it as a repo secret (paste the token when prompted):

   ```sh
   gh secret set CARGO_REGISTRY_TOKEN --repo bryceremick/rm-comments
   ```

Note: you already have a local token in `~/.cargo/credentials.toml`; minting a
separate CI-scoped token (as above) is the better practice — it can be revoked
without breaking your machine.

### 2. `TAP_GITHUB_TOKEN` — lets CI push the formula to `homebrew-tap`

The workflow's default token can only touch the `rm-comments` repo, so pushing to
`bryceremick/homebrew-tap` needs a personal access token:

1. <https://github.com/settings/personal-access-tokens/new> (fine-grained):
   - **Repository access**: Only select repositories → `bryceremick/homebrew-tap`
   - **Permissions**: Contents → **Read and write** (nothing else)
   - Expiry: your call — GitHub emails you before it lapses; regenerate and re-run
     `gh secret set` when it does.
2. Add it:

   ```sh
   gh secret set TAP_GITHUB_TOKEN --repo bryceremick/rm-comments
   ```

### 3. Leftover cleanup (optional)

The pre-rename repo still exists:

```sh
gh auth refresh -h github.com -s delete_repo
gh repo delete bryceremick/zed-strip-comments --yes
```

## Verifying a release

```sh
gh release view v<X.Y.Z>                        # binaries attached
cargo install rm-comments --force               # crates.io
brew update && brew upgrade rm-comments         # tap formula
cargo binstall rm-comments --force              # prebuilt via binstall
```

## Auth summary

| Surface | Credential | Where |
|---|---|---|
| GitHub Release | automatic `GITHUB_TOKEN` | nothing to manage |
| crates.io | `CARGO_REGISTRY_TOKEN` repo secret | one-time setup §1 |
| Homebrew tap | `TAP_GITHUB_TOKEN` repo secret | one-time setup §2 |
| binstall / docs.rs / plugin / Zed | none | — |
