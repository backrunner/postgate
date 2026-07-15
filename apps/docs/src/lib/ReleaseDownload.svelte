<script lang="ts">
  import { onMount } from 'svelte';
  import { FontAwesomeIcon } from '@fortawesome/svelte-fontawesome';
  import { faApple } from '@fortawesome/free-brands-svg-icons/faApple';
  import { faGithub } from '@fortawesome/free-brands-svg-icons/faGithub';
  import { faWindows } from '@fortawesome/free-brands-svg-icons/faWindows';
  import { Clock3, Download, LoaderCircle } from 'lucide-svelte';

  interface ReleaseAsset {
    name: string;
    browser_download_url: string;
    size: number;
  }

  interface GithubRelease {
    tag_name: string;
    name: string;
    html_url: string;
    published_at: string;
    assets: ReleaseAsset[];
  }

  type MacChannel = 'mac-arm' | 'mac-intel';
  type Platform = 'macos' | 'windows';

  export let locale: 'en' | 'zh' = 'en';

  const messages = {
    en: {
      stable: 'Stable channel',
      latest: 'Latest release',
      checkingAria: 'Checking latest release',
      available: 'Available',
      ready: 'Signed macOS builds are delivered directly through GitHub Releases.',
      empty: 'The first signed macOS release is being prepared.',
      error: 'We could not check GitHub right now. Open Releases to check manually.',
      loading: 'Checking GitHub for the newest signed macOS build.',
      notes: 'Release notes',
      platform: 'Platform',
      architecture: 'Architecture',
      availability: 'Availability',
      platformAria: 'Choose a download platform',
      macBuildsAria: 'Choose a macOS architecture',
      download: 'Download',
      viewReleases: 'View releases',
      appleSilicon: 'Apple silicon',
      comingSoon: 'Coming soon',
      windowsPreview: 'Windows builds are in preparation.'
    },
    zh: {
      stable: '稳定版',
      latest: '最新版本',
      checkingAria: '正在检查最新版本',
      available: '可下载',
      ready: '已签名的 macOS 安装包会直接通过 GitHub Releases 发布。',
      empty: '首个已签名的 macOS 版本正在准备中。',
      error: '暂时无法访问 GitHub，请前往 Releases 页面重试。',
      loading: '正在从 GitHub 获取最新 macOS 版本。',
      notes: '版本说明',
      platform: '平台',
      architecture: '架构',
      availability: '可用状态',
      platformAria: '选择下载平台',
      macBuildsAria: '选择 macOS 架构',
      download: '下载',
      viewReleases: '查看 Releases',
      appleSilicon: 'Apple 芯片',
      comingSoon: '敬请期待',
      windowsPreview: 'Windows 版本正在准备中。'
    }
  } as const;

  $: copy = messages[locale];

  let state: 'loading' | 'ready' | 'empty' | 'error' = 'loading';
  let release: GithubRelease | null = null;
  let selectedPlatform: Platform = 'macos';
  let selectedMac: MacChannel = 'mac-arm';

  $: asset = release ? selectAsset(release.assets, selectedMac) : undefined;
  $: releaseHref = release?.html_url ?? 'https://github.com/backrunner/postgate/releases';
  $: downloadHref = asset?.browser_download_url ?? releaseHref;

  onMount(() => {
    const platform = navigator.platform.toLowerCase();
    if (platform.includes('win')) selectedPlatform = 'windows';
    void loadLatestRelease();
  });

  async function loadLatestRelease() {
    state = 'loading';
    try {
      const response = await fetch('https://api.github.com/repos/backrunner/postgate/releases/latest', {
        headers: { Accept: 'application/vnd.github+json' }
      });
      if (response.status === 404) {
        state = 'empty';
        return;
      }
      if (!response.ok) throw new Error(`GitHub returned ${response.status}`);
      release = await response.json() as GithubRelease;
      state = 'ready';
    } catch {
      state = 'error';
    }
  }

  function selectAsset(assets: ReleaseAsset[], channel: MacChannel): ReleaseAsset | undefined {
    const candidates = assets.filter((candidate) => {
      const name = candidate.name.toLowerCase();
      return !name.endsWith('.sig') && name !== 'latest.json';
    });

    if (channel === 'mac-arm') {
      return candidates.find((candidate) =>
        candidate.name.toLowerCase().endsWith('.dmg') && /(aarch64|arm64)/i.test(candidate.name)
      );
    }
    return candidates.find((candidate) =>
      candidate.name.toLowerCase().endsWith('.dmg') && /(x64|x86_64)/i.test(candidate.name)
    );
  }

  function formatSize(bytes: number): string {
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

<div class="release-tool" data-state={state}>
  <div class="release-heading">
    <div>
      <p class="eyebrow">{copy.stable}</p>
      <div class="version-line">
        <h2>{release?.tag_name ?? copy.latest}</h2>
        {#if state === 'loading'}
          <span class="spin"><LoaderCircle size={18} aria-label={copy.checkingAria} /></span>
        {:else if state === 'ready'}
          <span class="live"><span></span>{copy.available}</span>
        {/if}
      </div>
      <p class="release-copy">
        {#if state === 'ready'}
          {copy.ready}
        {:else if state === 'empty'}
          {copy.empty}
        {:else if state === 'error'}
          {copy.error}
        {:else}
          {copy.loading}
        {/if}
      </p>
    </div>
    <a class="github-link" href={releaseHref} target="_blank" rel="noreferrer">
      <FontAwesomeIcon icon={faGithub} fixedWidth style="width: 17px; height: 17px;" />
      {copy.notes}
    </a>
  </div>

  <div class="release-actions">
    <div class="release-options">
      <div class="control-row">
        <span class="control-label">{copy.platform}</span>
        <div class="platform-switch" role="group" aria-label={copy.platformAria}>
          <button
            type="button"
            class:active={selectedPlatform === 'macos'}
            aria-pressed={selectedPlatform === 'macos'}
            on:click={() => selectedPlatform = 'macos'}
          >
            <FontAwesomeIcon icon={faApple} fixedWidth style="width: 17px; height: 17px;" />
            <span>macOS</span>
          </button>
          <button
            type="button"
            class:active={selectedPlatform === 'windows'}
            aria-pressed={selectedPlatform === 'windows'}
            on:click={() => selectedPlatform = 'windows'}
          >
            <FontAwesomeIcon icon={faWindows} fixedWidth style="width: 17px; height: 17px;" />
            <span>Windows</span>
          </button>
        </div>
      </div>

      <div class="control-row build-row">
        {#if selectedPlatform === 'macos'}
          <span class="control-label">{copy.architecture}</span>
          <div class="architecture-switch" role="group" aria-label={copy.macBuildsAria}>
            <button
              type="button"
              class:active={selectedMac === 'mac-arm'}
              aria-pressed={selectedMac === 'mac-arm'}
              on:click={() => selectedMac = 'mac-arm'}
            >{copy.appleSilicon}</button>
            <button
              type="button"
              class:active={selectedMac === 'mac-intel'}
              aria-pressed={selectedMac === 'mac-intel'}
              on:click={() => selectedMac = 'mac-intel'}
            >Intel</button>
          </div>
        {:else}
          <span class="control-label">{copy.availability}</span>
          <p class="availability-copy"><Clock3 size={15} />{copy.windowsPreview}</p>
        {/if}
      </div>
    </div>

    {#if selectedPlatform === 'windows'}
      <div class="download-button unavailable" role="status">
        <Clock3 size={18} />
        <strong>{copy.comingSoon}</strong>
      </div>
    {:else if asset}
      <a class="download-button asset-download" href={downloadHref} target="_blank" rel="noreferrer">
        <Download size={18} />
        <span>
          <strong>{copy.download}</strong>
          <small>{asset.name} · {formatSize(asset.size)}</small>
        </span>
      </a>
    {:else}
      <a class="download-button releases-only" href={releaseHref} target="_blank" rel="noreferrer" aria-label={copy.viewReleases}>
        <FontAwesomeIcon icon={faGithub} fixedWidth style="width: 18px; height: 18px;" />
        <strong>{copy.viewReleases}</strong>
      </a>
    {/if}
  </div>
</div>

<style>
  .release-tool {
    width: min(1120px, calc(100% - 2rem));
    margin: 0 auto;
    padding: 1.5rem;
    border: 1px solid var(--pg-glass-line);
    border-radius: 8px;
    background: var(--pg-glass);
    box-shadow: var(--pg-glass-shadow);
    backdrop-filter: blur(28px) saturate(155%);
  }

  .release-heading,
  .release-actions,
  .version-line,
  .github-link,
  .download-button,
  .platform-switch button,
  .architecture-switch button,
  .availability-copy {
    display: flex;
    align-items: center;
  }

  .release-heading,
  .release-actions {
    justify-content: space-between;
    gap: 1.5rem;
  }

  .eyebrow {
    margin: 0 0 .35rem;
    color: var(--pg-primary);
    font: 700 .72rem/1 var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0;
  }

  .version-line {
    gap: .8rem;
  }

  h2 {
    margin: 0;
    font: 650 1.65rem/1.2 var(--sd-font-display);
    letter-spacing: 0;
  }

  .live {
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    color: var(--pg-muted);
    font-size: .76rem;
  }

  .live span {
    width: .45rem;
    height: .45rem;
    border-radius: 50%;
    background: var(--pg-success);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--pg-success) 15%, transparent);
  }

  .release-copy {
    margin: .35rem 0 0;
    color: var(--pg-muted);
    font-size: .88rem;
  }

  .github-link {
    gap: .45rem;
    color: var(--pg-link);
    font-size: .84rem;
    text-decoration: none;
  }

  .release-actions {
    margin-top: 1.35rem;
    padding-top: 1.35rem;
    border-top: 1px solid var(--pg-line);
  }

  .release-options {
    display: grid;
    gap: .6rem;
    flex: 1;
    max-width: 31rem;
  }

  .control-row {
    display: grid;
    grid-template-columns: 5.5rem minmax(0, 1fr);
    align-items: center;
    gap: .85rem;
    min-height: 2.65rem;
  }

  .control-label {
    color: var(--pg-muted);
    font: 600 .68rem/1 var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0;
  }

  .platform-switch,
  .architecture-switch {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    padding: .25rem;
    border: 1px solid var(--pg-line);
    border-radius: 7px;
    background: color-mix(in srgb, var(--pg-surface) 75%, transparent);
  }

  .platform-switch button,
  .architecture-switch button {
    min-width: 0;
    min-height: 2.15rem;
    justify-content: center;
    gap: .5rem;
    padding: .45rem .75rem;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--pg-muted);
    font: 600 .78rem/1 var(--sd-font-sans);
    cursor: pointer;
    transition: background 160ms ease, color 160ms ease, transform 100ms ease-out;
  }

  .platform-switch button:active,
  .architecture-switch button:active,
  .download-button:not(.unavailable):active,
  .github-link:active {
    transform: scale(.98);
  }

  .platform-switch button.active,
  .architecture-switch button.active {
    background: var(--pg-ink);
    color: var(--pg-bg);
  }

  .download-button span {
    min-width: 0;
  }

  .download-button strong,
  .download-button small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: 0;
  }

  .download-button strong {
    font-size: .8rem;
  }

  .download-button small {
    margin-top: .1rem;
    color: var(--pg-muted);
    font-size: .66rem;
  }

  .availability-copy {
    min-height: 2.65rem;
    gap: .55rem;
    margin: 0;
    color: var(--pg-muted);
    font-size: .78rem;
  }

  .download-button {
    width: 15.5rem;
    min-height: 3.5rem;
    justify-content: center;
    gap: .7rem;
    padding: 0 1rem;
    border-radius: 6px;
    background: var(--pg-ink);
    color: var(--pg-bg);
    text-decoration: none;
    transition: opacity 160ms ease, transform 100ms ease-out;
  }

  .asset-download:hover {
    opacity: .88;
  }

  .download-button.unavailable {
    border: 1px solid var(--pg-line);
    background: color-mix(in srgb, var(--pg-surface) 72%, transparent);
    color: var(--pg-muted);
    cursor: default;
  }

  .download-button.releases-only {
    border: 1px solid var(--pg-line);
    background: transparent;
    color: var(--pg-ink);
  }

  .download-button.releases-only:hover {
    background: var(--pg-surface);
  }

  .download-button small {
    max-width: 11rem;
    color: color-mix(in srgb, var(--pg-bg) 68%, transparent);
  }

  .spin {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 820px) {
    .release-actions,
    .release-heading {
      align-items: stretch;
      flex-direction: column;
    }

    .release-options {
      max-width: none;
    }

    .download-button {
      width: auto;
    }

    .github-link {
      align-self: flex-start;
    }
  }

  @media (max-width: 580px) {
    .release-tool {
      width: calc(100% - 1rem);
      padding: 1rem;
    }

    .control-row {
      grid-template-columns: 1fr;
      gap: .4rem;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spin { animation: none; }
    .platform-switch button,
    .architecture-switch button,
    .download-button,
    .github-link { transition: none; }
  }

  @media (prefers-reduced-transparency: reduce) {
    .release-tool {
      background: var(--pg-surface);
      backdrop-filter: none;
    }
  }
</style>
