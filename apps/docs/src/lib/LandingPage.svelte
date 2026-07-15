<script lang="ts">
  import { ArrowRight, BookOpen, Bug, GitFork, Plug, Radar, Repeat2, ShieldCheck, SlidersHorizontal } from 'lucide-svelte';
  import { ThemeToggle } from 'svedocs/theme';
  import ReleaseDownload from '$lib/ReleaseDownload.svelte';

  export let locale: 'en' | 'zh' = 'en';

  const messages = {
    en: {
      title: 'PostGate - Local traffic, under control',
      description: 'Capture, rewrite, replay, and debug local web traffic with a fast Tauri desktop proxy.',
      homeAria: 'PostGate home',
      docs: 'Docs',
      rules: 'Rules',
      debug: 'Debug',
      download: 'Download',
      localeLabel: '中文',
      localeHref: '/zh',
      githubAria: 'PostGate on GitHub',
      kicker: 'Local traffic control',
      lede: 'See the request. Change the outcome.',
      detail: 'A desktop MITM proxy for frontend engineers who need precise capture, programmable rules, repeatable replay, and browser-level debugging.',
      getStarted: 'Get started',
      downloadDesktop: 'Download desktop',
      localFirst: 'Local-first',
      pluginReady: 'Plugin-ready',
      workflowIndex: '01 / Workflow',
      workflowTitle: 'One path from observation to proof.',
      workflowIntro: 'PostGate keeps capture, mutation, verification, and debugging in the same local workspace.',
      workflows: [
        ['Capture', 'Read every request, response, timing phase, and matched rule without leaving the desktop.'],
        ['Rewrite', 'Route hosts, replace files, edit bodies and headers, inject code, delay, throttle, or mock.'],
        ['Replay', 'Save requests into collections, edit them precisely, and execute them again.'],
        ['Debug', 'Inject a CDP bridge and collect console output, errors, Fetch, and XHR activity.']
      ],
      signalIndex: '02 / Signal',
      signalTitle: 'Readable under real traffic.',
      signalIntro: 'Dense request streams stay scannable. Status, protocol, timing, and rule outcomes keep their own visual language.',
      exploreCapture: 'Explore capture',
      requestTableAria: 'Example captured requests',
      method: 'Method',
      request: 'Request',
      result: 'Result',
      time: 'Time',
      rulesIndex: '03 / Rules',
      rulesTitle: 'Small rules. Immediate outcomes.',
      rulesIntro: 'Use Whistle-compatible patterns and actions to control routing, requests, responses, files, timing, browser injection, and plugins.',
      rulesGuide: 'Read the rules guide',
      ruleExampleAria: 'PostGate rule example',
      ruleCommentRoute: '# route the API to a local service',
      ruleCommentFile: '# replace a bundle and enable browser debug',
      closingKicker: 'Start with one captured request.',
      closingTitle: 'Make local traffic explain itself.',
      openDocs: 'Open the docs',
      viewSource: 'View source',
      footer: 'Local-first developer infrastructure. MIT licensed.',
      documentation: 'Documentation'
    },
    zh: {
      title: 'PostGate - 让本地流量清晰可控',
      description: '使用快速的 Tauri 桌面代理捕获、改写、重放和调试本地 Web 流量。',
      homeAria: 'PostGate 首页',
      docs: '文档',
      rules: '规则',
      debug: '调试',
      download: '下载',
      localeLabel: 'EN',
      localeHref: '/',
      githubAria: '在 GitHub 上查看 PostGate',
      kicker: '本地流量控制',
      lede: '看清请求，改变结果。',
      detail: '为前端工程师打造的桌面中间人代理，提供精确的流量捕获、可编程规则、可重复的请求重放和浏览器级调试。',
      getStarted: '快速开始',
      downloadDesktop: '下载桌面版',
      localFirst: '本地优先',
      pluginReady: '支持插件',
      workflowIndex: '01 / 工作流',
      workflowTitle: '从发现问题到验证结果，一条路径完成。',
      workflowIntro: '捕获、修改、验证和调试始终在同一个本地工作区中完成。',
      workflows: [
        ['捕获', '在桌面应用中查看每个请求、响应、耗时阶段和命中的规则。'],
        ['改写', '路由主机、替换文件、修改正文和请求头、注入代码，或模拟延迟与限速。'],
        ['重放', '把请求保存到集合中，精确修改后再次执行。'],
        ['调试', '注入 CDP 调试桥，收集 Console、页面错误、Fetch 和 XHR 活动。']
      ],
      signalIndex: '02 / 流量',
      signalTitle: '流量再多，也能快速读懂。',
      signalIntro: '密集的请求列表依然清晰可扫读，状态、协议、耗时和规则结果各有明确的视觉提示。',
      exploreCapture: '了解流量捕获',
      requestTableAria: '捕获请求示例',
      method: '方法',
      request: '请求',
      result: '结果',
      time: '耗时',
      rulesIndex: '03 / 规则',
      rulesTitle: '几行规则，立即生效。',
      rulesIntro: '使用 Whistle 兼容的匹配条件和操作，控制路由、请求、响应、文件、耗时、浏览器注入和插件。',
      rulesGuide: '阅读规则指南',
      ruleExampleAria: 'PostGate 规则示例',
      ruleCommentRoute: '# 将 API 路由到本地服务',
      ruleCommentFile: '# 替换前端文件并启用浏览器调试',
      closingKicker: '从捕获一个请求开始。',
      closingTitle: '让每一段本地流量都有迹可循。',
      openDocs: '打开文档',
      viewSource: '查看源码',
      footer: '本地优先的开发基础设施。MIT 许可。',
      documentation: '文档'
    }
  } as const;

  $: copy = messages[locale];
  $: workflows = [Radar, SlidersHorizontal, Repeat2, Bug].map((icon, index) => ({
    icon,
    label: copy.workflows[index][0],
    copy: copy.workflows[index][1]
  }));
  $: docsHref = locale === 'zh' ? '/docs/zh' : '/docs';
  $: rulesHref = locale === 'zh' ? '/docs/zh/rules' : '/docs/rules';
  $: debugHref = locale === 'zh' ? '/docs/zh/debug' : '/docs/debug';
  $: captureHref = locale === 'zh' ? '/docs/zh/capture' : '/docs/capture';
  $: canonicalUrl = locale === 'zh' ? 'https://postgate.alkinum.io/zh' : 'https://postgate.alkinum.io/';
  $: ogImageUrl = locale === 'zh'
    ? 'https://postgate.alkinum.io/og/zh.svg'
    : 'https://postgate.alkinum.io/og/index.svg';

  const requestRows = [
    { method: 'GET', path: '/api/cart', status: '200', time: '38 ms' },
    { method: 'POST', path: '/checkout', status: '201', time: '112 ms' },
    { method: 'GET', path: '/assets/app.js', status: 'LOCAL', time: '4 ms' },
    { method: 'GET', path: '/profile', status: 'DEBUG', time: '61 ms' }
  ];
</script>

<svelte:head>
  <title>{copy.title}</title>
  <meta name="description" content={copy.description} />
  <link rel="canonical" href={canonicalUrl} />
  <link rel="alternate" hreflang="en" href="https://postgate.alkinum.io/" />
  <link rel="alternate" hreflang="zh-CN" href="https://postgate.alkinum.io/zh" />
  <link rel="alternate" hreflang="x-default" href="https://postgate.alkinum.io/" />
  <meta property="og:type" content="website" />
  <meta property="og:title" content={copy.title} />
  <meta property="og:description" content={copy.description} />
  <meta property="og:url" content={canonicalUrl} />
  <meta property="og:image" content={ogImageUrl} />
</svelte:head>

<div class="landing">
  <header class="landing-nav">
    <a class="brand" href={locale === 'zh' ? '/zh' : '/'} aria-label={copy.homeAria}>
      <img src="/postgate.png" alt="" width="30" height="30" />
      <span>PostGate</span>
    </a>
    <nav aria-label="Primary navigation">
      <a href={docsHref}>{copy.docs}</a>
      <a href={rulesHref}>{copy.rules}</a>
      <a href={debugHref}>{copy.debug}</a>
      <a href="#download">{copy.download}</a>
    </nav>
    <div class="nav-actions">
      <a class="locale-link" href={copy.localeHref} lang={locale === 'zh' ? 'en' : 'zh-CN'}>{copy.localeLabel}</a>
      <a class="icon-link" href="https://github.com/backrunner/postgate" target="_blank" rel="noreferrer" aria-label={copy.githubAria} title="GitHub">
        <GitFork size={18} />
      </a>
      <ThemeToggle defaultMode="system" />
    </div>
  </header>

  <main>
    <section class="hero">
      <div class="traffic-scene" aria-hidden="true">
        <div class="scene-axis browser"><span>CLIENT</span></div>
        <div class="scene-axis gate"><img src="/postgate.png" alt="" /><span>POSTGATE</span></div>
        <div class="scene-axis upstream"><span>UPSTREAM</span></div>
        <div class="route-line line-a"></div>
        <div class="route-line line-b"></div>
        <div class="route-line line-c"></div>
        <div class="packet packet-a"><span>GET</span><b>/api/cart</b><em>200</em></div>
        <div class="packet packet-b"><span>POST</span><b>/checkout</b><em>201</em></div>
        <div class="packet packet-c"><span>GET</span><b>/app.js</b><em>FILE</em></div>
      </div>

      <div class="hero-copy">
        <div class="hero-kicker"><span></span>{copy.kicker}</div>
        <h1>PostGate</h1>
        <p class="hero-lede">{copy.lede}</p>
        <p class="hero-detail">{copy.detail}</p>
        <div class="hero-actions">
          <a class="primary-action" href={docsHref}>
            <BookOpen size={18} />
            {copy.getStarted}
            <ArrowRight size={17} />
          </a>
          <a class="secondary-action" href="#download">{copy.downloadDesktop}</a>
        </div>
        <div class="hero-proof" aria-label="PostGate capabilities">
          <span><ShieldCheck size={15} /> {copy.localFirst}</span>
          <span><Plug size={15} /> {copy.pluginReady}</span>
          <span><Radar size={15} /> HTTP/1.1 + HTTP/2</span>
        </div>
      </div>
    </section>

    <section class="release-band" id="download" aria-label="Download PostGate">
      <ReleaseDownload {locale} />
    </section>

    <section class="workflow-section">
      <div class="section-intro">
        <p class="section-index">{copy.workflowIndex}</p>
        <h2>{copy.workflowTitle}</h2>
        <p>{copy.workflowIntro}</p>
      </div>
      <div class="workflow-grid">
        {#each workflows as workflow, index}
          <article>
            <span class="workflow-number">0{index + 1}</span>
            <svelte:component this={workflow.icon} size={22} color="var(--pg-primary)" />
            <h3>{workflow.label}</h3>
            <p>{workflow.copy}</p>
          </article>
        {/each}
      </div>
    </section>

    <section class="capture-section">
      <div class="capture-copy">
        <p class="section-index">{copy.signalIndex}</p>
        <h2>{copy.signalTitle}</h2>
        <p>{copy.signalIntro}</p>
        <a href={captureHref}>{copy.exploreCapture} <ArrowRight size={16} /></a>
      </div>
      <div class="request-table" aria-label={copy.requestTableAria}>
        <div class="table-head"><span>{copy.method}</span><span>{copy.request}</span><span>{copy.result}</span><span>{copy.time}</span></div>
        {#each requestRows as row}
          <div class="request-row">
            <code data-method={row.method}>{row.method}</code><strong>{row.path}</strong><span data-result={row.status}>{row.status}</span><time>{row.time}</time>
          </div>
        {/each}
      </div>
    </section>

    <section class="rules-section">
      <div class="rule-editor" aria-label={copy.ruleExampleAria}>
        <div class="editor-bar"><span></span><span></span><span></span><b>local.rules</b></div>
        <pre><code><i>{copy.ruleCommentRoute}</i>
api.example.com <mark>host://127.0.0.1:3000</mark>

<i>{copy.ruleCommentFile}</i>
cdn.example.com/app.js <mark>file:///project/dist/app.js</mark>
example.com <mark>debug://</mark></code></pre>
      </div>
      <div class="rules-copy">
        <p class="section-index">{copy.rulesIndex}</p>
        <h2>{copy.rulesTitle}</h2>
        <p>{copy.rulesIntro}</p>
        <a href={rulesHref}>{copy.rulesGuide} <ArrowRight size={16} /></a>
      </div>
    </section>

    <section class="closing-section">
      <img src="/postgate.png" width="72" height="72" alt="PostGate" />
      <p>{copy.closingKicker}</p>
      <h2>{copy.closingTitle}</h2>
      <div>
        <a class="primary-action" href={docsHref}>{copy.openDocs} <ArrowRight size={17} /></a>
        <a class="secondary-action" href="https://github.com/backrunner/postgate" target="_blank" rel="noreferrer">{copy.viewSource}</a>
      </div>
    </section>
  </main>

  <footer class="landing-footer">
    <a class="brand" href={locale === 'zh' ? '/zh' : '/'}><img src="/postgate.png" alt="" width="26" height="26" /><span>PostGate</span></a>
    <p>{copy.footer}</p>
    <div><a href={docsHref}>{copy.documentation}</a><a href="https://github.com/backrunner/postgate">GitHub</a></div>
  </footer>
</div>

<style>
  :global(body) { overflow-x: hidden; }

  .landing {
    min-height: 100vh;
    background: var(--pg-bg);
    color: var(--pg-ink);
    font-family: var(--font-sans);
  }

  .landing-nav {
    position: fixed;
    z-index: 20;
    top: 1rem;
    left: 50%;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    width: min(1120px, calc(100% - 2rem));
    height: 3.5rem;
    padding: 0 .75rem 0 .9rem;
    border: 1px solid var(--pg-glass-line);
    border-radius: 8px;
    background: var(--pg-nav-glass);
    box-shadow: 0 10px 36px color-mix(in srgb, var(--pg-shadow) 20%, transparent);
    backdrop-filter: blur(24px) saturate(150%);
    transform: translateX(-50%);
  }

  .brand,
  .landing-nav nav,
  .nav-actions,
  .hero-actions,
  .hero-proof,
  .primary-action,
  .secondary-action,
  .icon-link,
  .capture-copy a,
  .rules-copy a,
  .landing-footer,
  .landing-footer div {
    display: flex;
    align-items: center;
  }

  .brand {
    width: max-content;
    gap: .55rem;
    color: var(--pg-ink);
    font-weight: 700;
    text-decoration: none;
  }

  .brand img { border-radius: 7px; }

  .landing-nav nav { gap: .25rem; }
  .landing-nav nav a,
  .secondary-action,
  .icon-link {
    color: var(--pg-muted);
    text-decoration: none;
  }

  .locale-link {
    padding: .5rem .55rem;
    color: var(--pg-muted);
    font-size: .76rem;
    text-decoration: none;
  }

  .locale-link:hover { color: var(--pg-ink); }

  .landing-nav nav a {
    padding: .55rem .7rem;
    border-radius: 5px;
    font-size: .82rem;
    transition: color 160ms ease, background 160ms ease;
  }

  .landing-nav nav a:hover { color: var(--pg-ink); background: var(--pg-glass-hover); }

  .nav-actions {
    justify-self: end;
    gap: .2rem;
  }

  .icon-link {
    justify-content: center;
    width: 2.35rem;
    height: 2.35rem;
    border-radius: 5px;
  }

  .hero {
    position: relative;
    display: flex;
    align-items: center;
    min-height: calc(100svh - 5rem);
    padding: 7rem max(1rem, calc((100% - 1120px) / 2)) 4rem;
    overflow: hidden;
    box-sizing: border-box;
  }

  .hero-copy {
    position: relative;
    z-index: 2;
    width: min(38rem, 58%);
  }

  .hero-kicker {
    display: flex;
    align-items: center;
    gap: .55rem;
    color: var(--pg-muted);
    font: 700 .76rem/1 var(--font-mono);
    text-transform: uppercase;
  }

  .hero-kicker span {
    width: .5rem;
    height: .5rem;
    border-radius: 50%;
    background: var(--pg-success);
    box-shadow: 0 0 0 5px color-mix(in srgb, var(--pg-success) 14%, transparent);
  }

  h1 {
    margin: 1.25rem 0 0;
    font: 700 4.5rem/.95 var(--sd-font-display);
    letter-spacing: 0;
  }

  .hero-lede {
    max-width: 35rem;
    margin: 1.25rem 0 0;
    font: 600 2rem/1.15 var(--sd-font-display);
    letter-spacing: 0;
  }

  .hero-detail {
    max-width: 35rem;
    margin: 1.35rem 0 0;
    color: var(--pg-muted);
    font-size: 1.04rem;
    line-height: 1.7;
  }

  .hero-actions {
    gap: .65rem;
    margin-top: 2rem;
  }

  .primary-action,
  .secondary-action {
    min-height: 2.85rem;
    justify-content: center;
    gap: .55rem;
    padding: 0 1rem;
    border-radius: 6px;
    font-size: .88rem;
    font-weight: 650;
    transition: transform 100ms ease-out, opacity 160ms ease, background 160ms ease;
  }

  .primary-action {
    background: var(--pg-ink);
    color: var(--pg-bg);
    text-decoration: none;
  }

  .secondary-action { border: 1px solid var(--pg-line); }
  .primary-action:hover { opacity: .88; }
  .secondary-action:hover { background: var(--pg-surface); color: var(--pg-ink); }
  .primary-action:active, .secondary-action:active, .icon-link:active { transform: scale(.97); }

  .hero-proof {
    flex-wrap: wrap;
    gap: 1rem;
    margin-top: 2rem;
    color: var(--pg-muted);
    font-size: .75rem;
  }

  .hero-proof span { display: inline-flex; align-items: center; gap: .35rem; }

  .traffic-scene {
    position: absolute;
    inset: 0 0 0 38%;
    overflow: hidden;
    opacity: .96;
  }

  .scene-axis {
    position: absolute;
    top: 6rem;
    bottom: 3rem;
    width: 1px;
    background: var(--pg-line);
  }

  .scene-axis span {
    position: absolute;
    top: 1rem;
    left: .7rem;
    color: var(--pg-faint);
    font: 700 .64rem/1 var(--font-mono);
  }

  .scene-axis.browser { left: 16%; }
  .scene-axis.gate { left: 53%; background: color-mix(in srgb, var(--pg-primary) 50%, var(--pg-line)); }
  .scene-axis.upstream { left: 88%; }
  .scene-axis.gate img { position: absolute; top: 45%; left: -1.4rem; width: 2.8rem; height: 2.8rem; border-radius: 8px; box-shadow: 0 12px 30px var(--pg-shadow); }

  .route-line {
    position: absolute;
    left: 16%;
    right: 12%;
    height: 1px;
    background: var(--pg-line);
  }

  .line-a { top: 34%; }
  .line-b { top: 51%; }
  .line-c { top: 68%; }

  .packet {
    position: absolute;
    left: 16%;
    display: grid;
    grid-template-columns: 3rem 1fr 3.2rem;
    align-items: center;
    width: 15rem;
    height: 2.4rem;
    padding: 0 .7rem;
    border: 1px solid var(--pg-glass-line);
    border-radius: 6px;
    background: var(--pg-packet);
    box-shadow: 0 10px 28px color-mix(in srgb, var(--pg-shadow) 20%, transparent);
    backdrop-filter: blur(16px);
    color: var(--pg-muted);
    font: .7rem/1 var(--font-mono);
    animation: packet-flow 7s linear infinite;
  }

  .packet b { overflow: hidden; color: var(--pg-ink); text-overflow: ellipsis; white-space: nowrap; }
  .packet span,
  .packet em { color: var(--pg-success); font-style: normal; text-align: right; }
  .packet-b span { color: var(--pg-info); }
  .packet-c em { color: var(--pg-info); }
  .packet-a { top: calc(34% - 1.2rem); }
  .packet-b { top: calc(51% - 1.2rem); animation-delay: -2.2s; }
  .packet-c { top: calc(68% - 1.2rem); animation-delay: -4.6s; }

  @keyframes packet-flow {
    0% { transform: translateX(-1rem); opacity: 0; }
    8% { opacity: 1; }
    88% { opacity: 1; }
    100% { transform: translateX(29rem); opacity: 0; }
  }

  .release-band {
    position: relative;
    z-index: 3;
    padding: 2.5rem 0 5.5rem;
    background: var(--pg-band);
    border-top: 1px solid var(--pg-line);
    border-bottom: 1px solid var(--pg-line);
  }

  .workflow-section,
  .capture-section,
  .rules-section,
  .closing-section,
  .landing-footer {
    width: min(1120px, calc(100% - 2rem));
    margin: 0 auto;
  }

  .workflow-section { padding: 7rem 0; }

  .section-intro {
    display: grid;
    grid-template-columns: 10rem minmax(18rem, 1fr) minmax(16rem, .8fr);
    gap: 2rem;
    align-items: end;
    padding-bottom: 3rem;
    border-bottom: 1px solid var(--pg-line);
  }

  .section-index {
    margin: 0;
    color: var(--pg-primary);
    font: 700 .72rem/1 var(--font-mono);
    text-transform: uppercase;
  }

  .section-intro h2,
  .capture-copy h2,
  .rules-copy h2,
  .closing-section h2 {
    margin: 0;
    font: 650 2.6rem/1.08 var(--sd-font-display);
    letter-spacing: 0;
  }

  .section-intro > p:last-child,
  .capture-copy > p:not(.section-index),
  .rules-copy > p:not(.section-index) {
    margin: 0;
    color: var(--pg-muted);
    line-height: 1.7;
  }

  .workflow-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
  }

  .workflow-grid article {
    position: relative;
    min-height: 15rem;
    padding: 2rem 1.5rem 1.5rem;
    border-right: 1px solid var(--pg-line);
  }

  .workflow-grid article:first-child { border-left: 1px solid var(--pg-line); }
  .workflow-number { position: absolute; top: 2rem; right: 1.5rem; color: var(--pg-faint); font: .7rem/1 var(--font-mono); }
  .workflow-grid h3 { margin: 2.2rem 0 .7rem; font-size: 1.15rem; letter-spacing: 0; }
  .workflow-grid p { margin: 0; color: var(--pg-muted); font-size: .88rem; line-height: 1.65; }

  .capture-section,
  .rules-section {
    display: grid;
    grid-template-columns: .75fr 1.25fr;
    align-items: center;
    gap: 5rem;
    padding: 7rem 0;
    border-top: 1px solid var(--pg-line);
  }

  .capture-copy h2,
  .rules-copy h2 { margin-top: 1.1rem; }
  .capture-copy > p:not(.section-index), .rules-copy > p:not(.section-index) { margin-top: 1.2rem; }
  .capture-copy a, .rules-copy a { width: max-content; gap: .4rem; margin-top: 1.5rem; color: var(--pg-link); font-size: .84rem; font-weight: 650; text-decoration: none; }

  .request-table { border-top: 1px solid var(--pg-line); border-bottom: 1px solid var(--pg-line); }
  .table-head, .request-row { display: grid; grid-template-columns: 5rem 1fr 5rem 4rem; align-items: center; gap: 1rem; }
  .table-head { min-height: 2.4rem; color: var(--pg-faint); font: 700 .64rem/1 var(--font-mono); text-transform: uppercase; }
  .request-row { min-height: 3.6rem; border-top: 1px solid var(--pg-line); font-size: .82rem; }
  .request-row code { color: var(--pg-muted); }
  .request-row code[data-method='GET'] { color: var(--pg-success); }
  .request-row code[data-method='POST'] { color: var(--pg-info); }
  .request-row code[data-method='PUT'],
  .request-row code[data-method='PATCH'] { color: var(--pg-warning); }
  .request-row code[data-method='DELETE'] { color: var(--pg-destructive); }
  .request-row strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .request-row span { color: var(--pg-success); font: 700 .7rem/1 var(--font-mono); }
  .request-row span[data-result='LOCAL'] { color: var(--pg-info); }
  .request-row span[data-result='DEBUG'] { color: var(--pg-warning); }
  .request-row time { color: var(--pg-muted); font: .72rem/1 var(--font-mono); text-align: right; }

  .rules-section { grid-template-columns: 1.25fr .75fr; }

  .rule-editor {
    overflow: hidden;
    border: 1px solid var(--pg-line);
    border-radius: 8px;
    background: var(--pg-code);
    box-shadow: 0 24px 60px color-mix(in srgb, var(--pg-shadow) 22%, transparent);
  }

  .editor-bar { display: flex; align-items: center; gap: .4rem; height: 2.7rem; padding: 0 .8rem; border-bottom: 1px solid var(--pg-line); }
  .editor-bar span { width: .55rem; height: .55rem; border-radius: 50%; background: var(--pg-line); }
  .editor-bar span:first-child { background: #ef6a5b; }
  .editor-bar span:nth-child(2) { background: #e7b84b; }
  .editor-bar span:nth-child(3) { background: #54b66e; }
  .editor-bar b { margin-left: .5rem; color: var(--pg-muted); font: 500 .7rem/1 var(--font-mono); }
  .rule-editor pre { margin: 0; padding: 1.5rem; overflow: auto; color: var(--pg-ink); font: .78rem/1.8 var(--font-mono); }
  .rule-editor i { color: var(--pg-editor-comment); font-style: normal; }
  .rule-editor mark { background: transparent; color: var(--pg-editor-action); }

  .closing-section {
    padding: 7rem 0;
    border-top: 1px solid var(--pg-line);
    text-align: center;
  }

  .closing-section img { margin: 0 auto; border-radius: 8px; }
  .closing-section > p { margin: 1.4rem 0 .7rem; color: var(--pg-primary); font: 700 .76rem/1 var(--font-mono); text-transform: uppercase; }
  .closing-section h2 { max-width: 40rem; margin: 0 auto; }
  .closing-section > div { display: flex; justify-content: center; gap: .65rem; margin-top: 2rem; }

  .landing-footer {
    justify-content: space-between;
    min-height: 6rem;
    border-top: 1px solid var(--pg-line);
    color: var(--pg-muted);
    font-size: .76rem;
  }

  .landing-footer p { margin: 0; }
  .landing-footer div { gap: 1rem; }
  .landing-footer div a { color: var(--pg-muted); text-decoration: none; }

  @media (max-width: 920px) {
    .traffic-scene { left: 28%; opacity: .45; }
    .hero-copy { width: min(38rem, 76%); }
    h1 { font-size: 3.5rem; }
    .hero-lede { font-size: 1.7rem; }
    .section-intro { grid-template-columns: 1fr 2fr; }
    .section-intro > p:last-child { grid-column: 2; }
    .workflow-grid { grid-template-columns: repeat(2, 1fr); }
    .workflow-grid article:nth-child(3) { border-left: 1px solid var(--pg-line); border-top: 1px solid var(--pg-line); }
    .workflow-grid article:nth-child(4) { border-top: 1px solid var(--pg-line); }
    .capture-section, .rules-section { gap: 3rem; }
  }

  @media (max-width: 720px) {
    .landing-nav { top: .5rem; grid-template-columns: 1fr auto; width: calc(100% - 1rem); }
    .landing-nav nav { display: none; }
    .hero { min-height: calc(100svh - 4rem); padding-top: 6rem; }
    .hero-copy { width: 100%; }
    .traffic-scene { inset: 0 0 0 25%; opacity: .2; }
    h1 { font-size: 2.8rem; }
    .hero-lede { font-size: 1.45rem; }
    .hero-detail { max-width: 30rem; font-size: .95rem; }
    .hero-proof { gap: .7rem; }
    .workflow-section, .capture-section, .rules-section, .closing-section { padding: 5rem 0; }
    .section-intro, .capture-section, .rules-section { grid-template-columns: 1fr; gap: 2rem; }
    .section-intro > p:last-child { grid-column: auto; }
    .section-intro h2, .capture-copy h2, .rules-copy h2, .closing-section h2 { font-size: 2rem; }
    .rules-copy { grid-row: 1; }
    .landing-footer { align-items: flex-start; flex-direction: column; justify-content: center; gap: .6rem; padding: 1.5rem 0; }
  }

  @media (max-width: 520px) {
    .hero-actions, .closing-section > div { align-items: stretch; flex-direction: column; }
    .hero-actions a, .closing-section a { width: auto; }
    .workflow-grid { grid-template-columns: 1fr; }
    .workflow-grid article, .workflow-grid article:nth-child(3) { border-left: 1px solid var(--pg-line); border-top: 1px solid var(--pg-line); }
    .workflow-grid article:first-child { border-top: 0; }
    .table-head, .request-row { grid-template-columns: 4rem 1fr 4rem; }
    .table-head span:last-child, .request-row time { display: none; }
    .landing-footer div { flex-wrap: wrap; }
  }

  @media (prefers-reduced-motion: reduce) {
    .packet { animation: none; opacity: .85; }
    .packet-a { transform: translateX(2rem); }
    .packet-b { transform: translateX(10rem); }
    .packet-c { transform: translateX(18rem); }
    .primary-action, .secondary-action, .icon-link, .landing-nav nav a { transition: none; }
  }

  @media (prefers-reduced-transparency: reduce) {
    .landing-nav, .packet { background: var(--pg-surface); backdrop-filter: none; }
  }
</style>
