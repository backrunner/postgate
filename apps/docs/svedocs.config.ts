import { defineConfig } from 'svedocs/config';

export default defineConfig({
  site: {
    name: 'PostGate',
    title: 'PostGate Documentation',
    description: 'Capture, rewrite, replay, and debug local web traffic with PostGate.',
    url: 'https://postgate.alkinum.io'
  },
  content: {
    root: 'content',
    docs: 'content/docs',
    pages: 'content/pages'
  },
  build: {
    mode: 'static'
  },
  theme: {
    defaultMode: 'system',
    palette: {
      accent: '#18181b',
      neutral: 'zinc'
    },
    fonts: {
      sans: 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      mono: 'ui-monospace, "SFMono-Regular", "Cascadia Code", monospace',
      display: 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    },
    radius: '8px',
    codeTheme: {
      light: 'github-light',
      dark: 'github-dark'
    },
    code: {
      lineNumbers: false,
      wrap: false,
      copyButton: true
    },
    brand: {
      label: 'PostGate',
      href: '/',
      logo: '/postgate.png',
      mark: false
    },
    nav: [
      { label: 'Get Started', labelKey: 'postgate.nav.start', href: '/docs' },
      { label: 'Rules', labelKey: 'postgate.nav.rules', href: '/docs/rules' },
      { label: 'Debug', labelKey: 'postgate.nav.debug', href: '/docs/debug' },
      { label: 'Plugins', labelKey: 'postgate.nav.plugins', href: '/docs/plugins' },
      { label: 'Download', labelKey: 'postgate.nav.download', href: '/#download' }
    ],
    social: [
      { label: 'GitHub', href: 'https://github.com/backrunner/postgate', external: true }
    ],
    footer: {
      text: 'PostGate is local-first developer infrastructure.',
      links: [
        { label: 'GitHub', href: 'https://github.com/backrunner/postgate', external: true },
        { label: 'MIT License', href: 'https://github.com/backrunner/postgate/blob/main/LICENSE', external: true }
      ]
    }
  },
  search: {
    enabled: true,
    provider: 'local',
    scope: 'current'
  },
  ai: false,
  i18n: {
    defaultLocale: 'en',
    locales: [
      { code: 'en', label: 'English', hreflang: 'en', dir: 'ltr' },
      { code: 'zh', label: '中文', hreflang: 'zh-CN', dir: 'ltr' }
    ],
    messages: {
      zh: {
        'nav.primary': '主导航',
        'nav.docs': '文档',
        'nav.documentation': '文档导航',
        'nav.footer': '页脚',
        'nav.social': '社交链接',
        'nav.mobile.open': '打开菜单',
        'nav.mobile.close': '关闭菜单',
        'nav.skipToContent': '跳到正文',
        'scope.locale': '语言',
        'scope.localeOptions': '语言选项',
        'scope.langShort': '语言',
        'search.trigger': '搜索',
        'search.dialog': '搜索文档',
        'search.query': '搜索关键词',
        'search.placeholder': '搜索文档',
        'search.results': '搜索结果',
        'search.loading': '正在搜索...',
        'search.loadingIndex': '正在加载搜索索引...',
        'search.indexError': '无法加载搜索索引。',
        'search.empty': '没有匹配的文档。',
        'toc.label': '本页内容',
        'heading.anchor': '链接到此章节',
        'article.kind.doc': '文档',
        'article.kind.page': '页面',
        'article.breadcrumb': '面包屑',
        'article.updated': '更新于 {date}',
        'article.edit': '编辑此页',
        'article.previous': '上一页',
        'article.next': '下一页',
        'code.copy': '复制代码',
        'code.copied': '已复制',
        'theme.switch': '切换到{mode}主题',
        'theme.light': '浅色',
        'theme.dark': '深色',
        'tools.label': '页面工具',
        'tools.backToTop': '回到顶部',
        'footer.text': '本地优先的开发基础设施。',
        'postgate.nav.start': '快速开始',
        'postgate.nav.rules': '规则',
        'postgate.nav.debug': '调试',
        'postgate.nav.plugins': '插件',
        'postgate.nav.download': '下载',
        'error.notFound.title': '页面未找到',
        'error.notFound.description': '这个地址没有对应的页面。',
        'error.backToDocs': '返回文档'
      }
    }
  },
  checks: {
    translations: true,
    assets: true,
    externalLinks: false
  },
  cloudflare: {
    compatibilityDate: '2026-07-15'
  },
  source: {
    editBaseUrl: 'https://github.com/backrunner/postgate/edit/main/apps/docs'
  },
  seo: {
    sitemap: true,
    robots: true,
    defaultAuthor: 'PostGate contributors',
    ogImage: {
      template: 'default',
      format: 'svg',
      outDir: 'static/og',
      renderer: 'svg'
    }
  }
});
