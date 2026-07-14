# Cloudflare deployment

The PostGate documentation site is a static svedocs build deployed directly to Cloudflare Pages. GitHub Actions is not part of the deployment path.

## First deployment

```bash
pnpm --filter @postgate/docs cloudflare:whoami
pnpm --filter @postgate/docs cloudflare:create
pnpm --filter @postgate/docs deploy
```

The project name is `postgate-docs`, the production branch is `main`, and `wrangler.jsonc` pins deployment to the Cloudflare account named `Alkinum`.

## Custom domain

After the first deployment, add `postgate.alkinum.io` under **Workers & Pages → postgate-docs → Custom domains** in the Cloudflare dashboard. Because `alkinum.io` is already managed by Cloudflare, Pages creates and manages the required DNS record and certificate.

Verify these URLs after the certificate becomes active:

- `https://postgate.alkinum.io/`
- `https://postgate.alkinum.io/docs`
- `https://postgate.alkinum.io/docs/zh`
- `https://postgate.alkinum.io/sitemap.xml`
- `https://postgate.alkinum.io/robots.txt`

## Later deployments

Run `pnpm --filter @postgate/docs deploy` from a clean, reviewed revision. The command checks and builds the static site before Wrangler uploads it to the production branch.

Use `pnpm --filter @postgate/docs deploy:preview` for a non-production preview deployment.

## GitHub release downloads

The landing page reads `https://api.github.com/repos/backrunner/postgate/releases/latest` in the browser and links directly to each asset's `browser_download_url`. This works for anonymous visitors only when the repository and release are public. The repository is currently private, so the download area will show its empty-release fallback until public releases are accessible.

Do not add a GitHub token to the frontend or to a public Pages variable. If releases must remain private, downloads require an authenticated server-side design and are outside the current public documentation-site flow.
