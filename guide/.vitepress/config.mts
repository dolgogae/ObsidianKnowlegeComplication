import { defineConfig } from "vitepress";

export default defineConfig({
  lang: "ko-KR",
  title: "OKC Guides",
  titleTemplate: ":title · OKC Guides",
  description:
    "여러 Obsidian Vault를 검증 가능한 하나의 Vault로 컴파일하는 방법",
  appearance: "dark",
  cleanUrls: true,
  lastUpdated: true,
  head: [
    ["meta", { name: "theme-color", content: "#173f2a" }],
    ["meta", { name: "color-scheme", content: "light dark" }],
  ],
  themeConfig: {
    nav: [
      { text: "V3 Integration", link: "/v3-integration" },
      { text: "V2 Regression", link: "/" },
      { text: "CLI", link: "/cli" },
      { text: "TUI", link: "/tui" },
      { text: "AI Provider", link: "/ai-provider" },
      {
        text: "v0.3.0-dev",
        items: [
          { text: "현재 구현 상태", link: "/current-state" },
          {
            text: "GitHub 저장소",
            link: "https://github.com/dolgogae/okc",
          },
        ],
      },
    ],
    sidebar: [
      {
        text: "시작하기",
        items: [
          { text: "V3 AI 통합", link: "/v3-integration" },
          { text: "V2 회귀 Quickstart", link: "/" },
        ],
      },
      {
        text: "인터페이스",
        items: [
          { text: "CLI 사용법", link: "/cli" },
          { text: "TUI 사용법", link: "/tui" },
        ],
      },
      {
        text: "워크플로우",
        items: [
          { text: "충돌 검토", link: "/conflicts" },
          { text: "AI Provider", link: "/ai-provider" },
          { text: "문제 해결", link: "/troubleshooting" },
        ],
      },
      {
        text: "프로젝트",
        items: [{ text: "현재 구현 상태", link: "/current-state" }],
      },
    ],
    outline: {
      level: [2, 3],
      label: "이 페이지",
    },
    search: {
      provider: "local",
    },
    socialLinks: [
      { icon: "github", link: "https://github.com/dolgogae/okc" },
    ],
    editLink: {
      pattern: "https://github.com/dolgogae/okc/edit/main/guide/:path",
      text: "GitHub에서 이 페이지 편집",
    },
    lastUpdated: {
      text: "마지막 업데이트",
      formatOptions: {
        dateStyle: "long",
      },
    },
    docFooter: {
      prev: "이전 가이드",
      next: "다음 가이드",
    },
    returnToTopLabel: "맨 위로",
    sidebarMenuLabel: "가이드 메뉴",
    darkModeSwitchLabel: "테마",
    lightModeSwitchTitle: "밝은 테마로 전환",
    darkModeSwitchTitle: "어두운 테마로 전환",
    footer: {
      message: "MIT OR Apache-2.0 · V3는 아직 개발 중이며 안정 릴리스가 아닙니다.",
      copyright: "Obsidian Knowledge Compilation",
    },
  },
});
