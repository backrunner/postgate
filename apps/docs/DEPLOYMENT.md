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

The landing page reads `https://api.github.com/repos/backrunner/postgate/releases?per_page=20` in the browser, selects the newest published release that contains a supported installer, and links the download button directly to the asset's `browser_download_url`. This intentionally includes prereleases because the public beta channel may be the newest downloadable build even when no stable release exists.

Do not add a GitHub token to the frontend or to a public Worker variable. The repository and downloadable release assets must remain publicly readable for anonymous website downloads.
