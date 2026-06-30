// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// GitHub project pages are served from a sub-path. The repo is
// OrekGames/track-cli, so the site lives at
// https://orekgames.github.io/track-cli/
//
// If a custom domain is ever added: drop `base`, set `site` to the domain,
// and add `website/public/CNAME`.
export default defineConfig({
  site: "https://orekgames.github.io",
  base: "/track-cli/",
  integrations: [
    starlight({
      title: "track",
      description:
        "One CLI for every issue tracker — YouTrack, Jira, GitHub, GitLab, and Linear.",
      customCss: ["./src/styles/starlight-custom.css"],
      social: {
        github: "https://github.com/OrekGames/track-cli",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Overview", slug: "overview" },
            { label: "Installation", slug: "installation" },
            { label: "Quick Start", slug: "quick-start" },
            { label: "Configuration", slug: "configuration" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Commands", slug: "commands" },
            { label: "Query Syntax", slug: "query-syntax" },
            { label: "Backends", slug: "backends" },
          ],
        },
        {
          label: "Guides",
          items: [{ label: "For AI Agents", slug: "ai-agents" }],
        },
      ],
    }),
  ],
});
