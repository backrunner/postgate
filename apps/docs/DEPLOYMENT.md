# Cloudflare Workers deployment

The PostGate documentation site is a static svedocs build deployed as Cloudflare Worker static assets. GitHub Actions is not part of the deployment path.

## First deployment

```bash
pnpm --filter @postgate/docs cloudflare:whoami
pnpm --filter @postgate/docs deploy:check
pnpm --filter @postgate/docs deploy
```

The Worker name is `postgate-docs`, and `wrangler.jsonc` pins deployment to the Cloudflare account named `Alkinum`.

## Custom domain

Wrangler binds `postgate.alkinum.io` as a Worker custom domain during deployment. `workers_dev` and preview URLs are disabled, so the documentation site is served only from the custom domain.

Verify these URLs after the certificate becomes active:

- `https://postgate.alkinum.io/`
- `https://postgate.alkinum.io/docs`
- `https://postgate.alkinum.io/docs/zh`
- `https://postgate.alkinum.io/sitemap.xml`
- `https://postgate.alkinum.io/robots.txt`

## Later deployments

Run `pnpm --filter @postgate/docs deploy` from a clean, reviewed revision. The command checks and builds the static site before Wrangler deploys it to the configured custom domain.

Run `pnpm --filter @postgate/docs deploy:check` to validate a deployment locally without uploading it.

## GitHub release downloads

The landing page reads `https://api.github.com/repos/backrunner/postgate/releases/latest` in the browser and links directly to each asset's `browser_download_url`. This works for anonymous visitors only when the repository and release are public. The repository is currently private, so the download area will show its empty-release fallback until public releases are accessible.

Do not add a GitHub token to the frontend or to a public Pages variable. If releases must remain private, downloads require an authenticated server-side design and are outside the current public documentation-site flow.
