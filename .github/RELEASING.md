# Releases

The release workflow uses [release-plz](https://release-plz.dev/) and the [shared x52 release tools](https://github.com/x52dev/nix/tree/main/release-tools). It runs on each push to `main`.

In the repository's **Settings > Actions > General**, enable **Allow GitHub Actions to create and approve pull requests**. The workflow uses `GITHUB_TOKEN` to manage draft release PRs. CI runs when a maintainer marks the PR ready for review.

## Registry setup

Configure a [GitHub trusted publisher](https://crates.io/docs/trusted-publishing) on crates.io for each workspace crate:

- `actix-client-ip-cloudflare`
- `actix-hash`
- `actix-proxy-protocol`
- `actix-web-lab`
- `actix-web-lab-derive`
- `collectools`
- `err-report`
- `proxyproto`
- `russe`

Use these publisher settings:

- Repository owner: `robjtede`
- Repository name: `actix-web-lab`
- Workflow filename: `release.yml`
- Environment: leave empty

Each crate must have a published version before it can use trusted publishing. The workflow obtains a temporary crates.io token through GitHub OIDC.

## Release process

1. Add release notes to the crate's `## Unreleased` changelog section as changes are merged.
2. Review the draft release PR that release-plz creates. The x52 tools move the unreleased notes under the new version and update README version links.
3. Mark the PR ready for review to run CI. Check the versions and changelogs, then merge the PR after CI passes.
4. The workflow publishes the crates, creates tags and GitHub releases, and copies the changelog sections into the release notes.

The workflow can also publish a version already prepared on `main` if it is not yet on crates.io.

The existing tag names are preserved. `actix-web-lab` and `actix-web-lab-derive` use the same version group. The derive crate has no changelog, so the x52 tools skip its changelog and release-note steps.
