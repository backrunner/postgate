<script lang="ts">
  import { onMount } from 'svelte';
  import { FontAwesomeIcon } from '@fortawesome/svelte-fontawesome';
  import { faApple } from '@fortawesome/free-brands-svg-icons/faApple';
  import { faWindows } from '@fortawesome/free-brands-svg-icons/faWindows';
  import { Clock3, Download, GitFork, LoaderCircle } from 'lucide-svelte';

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

  type Channel = 'mac-arm' | 'mac-intel' | 'windows';

  export let locale: 'en' | 'zh' = 'en';

  const messages = {
    en: {
      stable: 'Stable channel',
      latest: 'Latest release',
      checkingAria: 'Checking latest release',
      available: 'available',
      ready: 'Signed macOS builds, published directly from GitHub Releases.',
      empty: 'The first signed macOS release is being prepared.',
      error: 'GitHub is unavailable right now. Open Releases to try again.',
      loading: 'Checking GitHub for the newest signed macOS build.',
      notes: 'Release notes',
      pickerAria: 'Select download platform',
      download: 'Download',
      viewReleases: 'View releases',
      releases: 'GitHub Releases',
      appleSilicon: 'Apple silicon',
      comingSoon: 'Coming soon',
      windowsPreview: 'Windows builds are in preparation.'
    },
    zh: {
      stable: '稳定版',
      latest: '最新版本',
      checkingAria: '正在检查最新版本',
      available: '可下载',
      ready: '签名 macOS 安装包，由 GitHub Releases 直接提供。',
      empty: '首个签名 macOS 版本正在准备中。',
      error: '暂时无法访问 GitHub，请前往 Releases 页面重试。',
      loading: '正在从 GitHub 检查最新签名 macOS 版本。',
      notes: '版本说明',
      pickerAria: '选择下载平台',
      download: '下载',
      viewReleases: '查看 Releases',
      releases: 'GitHub Releases',
      appleSilicon: 'Apple 芯片',
      comingSoon: '敬请期待',
      windowsPreview: 'Windows 版本正在准备中。'
    }
  } as const;

  $: copy = messages[locale];
  $: channels = [
    { id: 'mac-arm' as const, label: 'macOS', detail: copy.appleSilicon },
    { id: 'mac-intel' as const, label: 'macOS', detail: 'Intel' },
    { id: 'windows' as const, label: 'Windows', detail: copy.comingSoon }
  ] satisfies Array<{ id: Channel; label: string; detail: string }>;

  let state: 'loading' | 'ready' | 'empty' | 'error' = 'loading';
  let release: GithubRelease | null = null;
  let selected: Channel = 'mac-arm';

  $: asset = release ? selectAsset(release.assets, selected) : undefined;
  $: releaseHref = release?.html_url ?? 'https://github.com/backrunner/postgate/releases';
  $: downloadHref = asset?.browser_download_url ?? releaseHref;

  onMount(() => {
    const platform = navigator.platform.toLowerCase();
    if (platform.includes('win')) selected = 'windows';
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

  function selectAsset(assets: ReleaseAsset[], channel: Channel): ReleaseAsset | undefined {
    const candidates = assets.filter((candidate) => {
      const name = candidate.name.toLowerCase();
      return !name.endsWith('.sig') && name !== 'latest.json';
    });

    if (channel === 'mac-arm') {
      return candidates.find((candidate) =>
        candidate.name.toLowerCase().endsWith('.dmg') && /(aarch64|arm64)/i.test(candidate.name)
      );
    }
    if (channel === 'mac-intel') {
      return candidates.find((candidate) =>
        candidate.name.toLowerCase().endsWith('.dmg') && /(x64|x86_64)/i.test(candidate.name)
      );
    }
    return undefined;
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
      <GitFork size={17} />
      {copy.notes}
    </a>
  </div>

  <div class="release-actions">
    <div class="channel-picker" aria-label={copy.pickerAria}>
      {#each channels as channel}
        <button
          type="button"
          class:active={selected === channel.id}
          class:upcoming={channel.id === 'windows'}
          aria-pressed={selected === channel.id}
          on:click={() => selected = channel.id}
        >
          {#if channel.id === 'windows'}
            <FontAwesomeIcon icon={faWindows} fixedWidth style="width: 17px; height: 17px; color: #0078d4;" />
          {:else}
            <FontAwesomeIcon icon={faApple} fixedWidth style="width: 17px; height: 17px;" />
          {/if}
          <span><strong>{channel.label}</strong><small>{channel.detail}</small></span>
        </button>
      {/each}
    </div>

    {#if selected === 'windows'}
      <div class="download-button unavailable" role="status">
        <Clock3 size={18} />
        <span>
          <strong>{copy.comingSoon}</strong>
          <small>{copy.windowsPreview}</small>
        </span>
      </div>
    {:else}
      <a class="download-button" href={downloadHref} target="_blank" rel="noreferrer">
        <Download size={18} />
        <span>
          <strong>{asset ? copy.download : copy.viewReleases}</strong>
          <small>{asset ? `${asset.name} · ${formatSize(asset.size)}` : copy.releases}</small>
        </span>
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
  .channel-picker button {
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

  .channel-picker {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    flex: 1;
    max-width: 34rem;
    padding: .25rem;
    border: 1px solid var(--pg-line);
    border-radius: 7px;
    background: color-mix(in srgb, var(--pg-surface) 75%, transparent);
  }

  .channel-picker button {
    min-width: 0;
    gap: .5rem;
    padding: .65rem .75rem;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--pg-muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 160ms ease, color 160ms ease, box-shadow 160ms ease, transform 100ms ease-out;
  }

  .channel-picker button:active,
  .download-button:not(.unavailable):active,
  .github-link:active {
    transform: scale(.98);
  }

  .channel-picker button.active {
    background: var(--pg-surface);
    color: var(--pg-ink);
    box-shadow: 0 1px 8px color-mix(in srgb, var(--pg-shadow) 45%, transparent);
  }

  .channel-picker button.upcoming small {
    color: var(--pg-warning);
  }

  .channel-picker span,
  .download-button span {
    min-width: 0;
  }

  .channel-picker strong,
  .channel-picker small,
  .download-button strong,
  .download-button small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: 0;
  }

  .channel-picker strong,
  .download-button strong {
    font-size: .8rem;
  }

  .channel-picker small,
  .download-button small {
    margin-top: .1rem;
    color: var(--pg-muted);
    font-size: .66rem;
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

  .download-button:not(.unavailable):hover {
    opacity: .88;
  }

  .download-button.unavailable {
    border: 1px solid var(--pg-line);
    background: color-mix(in srgb, var(--pg-surface) 72%, transparent);
    color: var(--pg-muted);
    cursor: default;
  }

  .download-button.unavailable small {
    color: var(--pg-muted);
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

    .channel-picker {
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

    .channel-picker {
      grid-template-columns: 1fr;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spin { animation: none; }
    .channel-picker button,
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
