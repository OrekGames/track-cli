---
title: Backends
description: Backend-specific behavior, capabilities, configuration, and examples for YouTrack, Jira, GitHub, GitLab, and Linear.
---

`track` keeps the day-to-day commands consistent, but each backend has different
authentication, identifier rules, search syntax, and capability boundaries. Use
this page to decide which backend mode you are in, what the CLI can do there,
and which backend-native quirks matter before automating a workflow.

## At a glance

<div class="backend-card-grid">
  <article class="backend-card">
    <h3>YouTrack</h3>
    <p>Full tracker automation for issues, fields, tags, links, and Knowledge Base pages.</p>
    <p class="backend-card-meta">IDs: <code>PROJ-123</code>, articles like <code>KB-A-1</code></p>
  </article>
  <article class="backend-card">
    <h3>Jira + Confluence</h3>
    <p>Best for Jira Cloud teams that also want Markdown-authored documentation published into Confluence pages.</p>
    <p class="backend-card-meta">IDs: issue keys, numeric Confluence page/space IDs</p>
  </article>
  <article class="backend-card">
    <h3>GitHub</h3>
    <p>Best for repository-scoped issue workflows, labels, milestones, comments, sub-issues, and Markdown wiki pages.</p>
    <p class="backend-card-meta">IDs: numeric issue numbers like <code>42</code></p>
  </article>
  <article class="backend-card">
    <h3>GitLab</h3>
    <p>Best for project-scoped GitLab issues, labels, notes, issue links, parent-child work items, and Markdown wiki pages.</p>
    <p class="backend-card-meta">IDs: project IID like <code>#42</code></p>
  </article>
  <article class="backend-card">
    <h3>Linear</h3>
    <p>Best for team-scoped Linear issues with labels, states, parent-child issues, and relation links.</p>
    <p class="backend-card-meta">IDs: team issue IDs like <code>ORE-123</code></p>
  </article>
</div>

## Feature + command compatibility

<p class="backend-matrix-intro">
  The shared command names are the selling point. The matrix shows where that
  surface stays consistent and where the backend's native model changes the
  behavior.
</p>

<div class="backend-matrix-legend" aria-label="Compatibility legend">
  <span><strong class="backend-status-token" data-status="full">Full</strong> native or complete support</span>
  <span><strong class="backend-status-token" data-status="limited">Limited</strong> command works with backend-specific limits</span>
  <span><strong class="backend-status-token" data-status="none">No</strong> unsupported or intentionally empty</span>
</div>

<section class="backend-feature-matrix" aria-label="Backend feature and command compatibility">
  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Issue lifecycle</h3>
      <p>Create, inspect, update, close, or delete tracked work items.</p>
      <div class="backend-feature-command"><code>track i c</code><code>track i u</code><code>track i rm</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>CRUD</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>CRUD</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitHub</span><strong>Limited</strong><em>no delete</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitLab</span><strong>Full</strong><em>CRUD</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Linear</span><strong>Full</strong><em>CRUD</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Search and batch reads</h3>
      <p>Fetch large result sets for reports, cleanup, triage, and agent context.</p>
      <div class="backend-feature-command"><code>track i s "..." --all</code><code>track -o json</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>query language</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>JQL</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitHub</span><strong>Full</strong><em>search API</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitLab</span><strong>Full</strong><em>filters</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Linear</span><strong>Full</strong><em>GraphQL</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Comments and history</h3>
      <p>Pull conversation context and field transition history into the same CLI output.</p>
      <div class="backend-feature-command"><code>track i cmt</code><code>track i hist</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>comments + history</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>comments + changelog</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitHub</span><strong>Full</strong><em>comments + events</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitLab</span><strong>Full</strong><em>notes + events</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Linear</span><strong>Full</strong><em>comments + history</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Links and hierarchy</h3>
      <p>Connect work across blockers, related issues, subtasks, and parent-child relationships.</p>
      <div class="backend-feature-command"><code>track i link A B</code><code>track i link A B -t subtask</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>native links</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>links + subtasks</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitHub</span><strong>Limited</strong><em>sub-issues only</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitLab</span><strong>Full</strong><em>links + parent</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Linear</span><strong>Full</strong><em>relations + parent</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Tags and labels</h3>
      <p>Normalize tracker labels as CLI tags for filtering, reporting, and updates.</p>
      <div class="backend-feature-command"><code>track tags ls</code><code>track i u --tag</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>tag objects</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>Jira</span><strong>Limited</strong><em>labels auto-create</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitHub</span><strong>Full</strong><em>labels</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitLab</span><strong>Full</strong><em>labels</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Linear</span><strong>Full</strong><em>labels</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Projects and scope</h3>
      <p>List or resolve the container that gives issue commands their default scope.</p>
      <div class="backend-feature-command"><code>track p ls</code><code>track p g</code><code>track p c</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>projects</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>Jira</span><strong>Limited</strong><em>no create</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitHub</span><strong>Limited</strong><em>repo scope</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitLab</span><strong>Limited</strong><em>project scope</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>Linear</span><strong>Limited</strong><em>team scope</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Fields</h3>
      <p>Read or update typed issue metadata while preserving backend-specific field names.</p>
      <div class="backend-feature-command"><code>track p fields PROJ</code><code>track i u --field</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>field admin</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>system + custom</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitHub</span><strong>Limited</strong><em>mapped fields</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitLab</span><strong>Limited</strong><em>standard fields</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>Linear</span><strong>Limited</strong><em>mapped fields</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Wiki and docs</h3>
      <p>Use one article command surface for tracker-native docs and repository wiki pages.</p>
      <div class="backend-feature-command"><code>track wiki c --body-file</code><code>track wiki u</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>Knowledge Base</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>Markdown to Confluence</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitHub</span><strong>Full</strong><em>Markdown wiki</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitLab</span><strong>Limited</strong><em>no comments or move</em></div>
      <div class="backend-feature-cell" data-status="none"><span>Linear</span><strong>No</strong><em>unsupported</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Attachments</h3>
      <p>Upload supporting files to issues, issue comments, wiki pages, or wiki comments where the backend exposes it.</p>
      <div class="backend-feature-command"><code>track i attach</code><code>track wiki attach</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>issue + wiki</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>issue + wiki</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitHub</span><strong>Limited</strong><em>wiki only</em></div>
      <div class="backend-feature-cell" data-status="limited"><span>GitLab</span><strong>Limited</strong><em>issue + wiki block</em></div>
      <div class="backend-feature-cell" data-status="none"><span>Linear</span><strong>No</strong><em>unsupported</em></div>
    </div>
  </article>

  <article class="backend-feature-row">
    <div class="backend-feature-head">
      <h3>Agent workflows</h3>
      <p>Return structured output, cache context, and preview declarative changes before mutation.</p>
      <div class="backend-feature-command"><code>track -o json</code><code>track cache refresh</code><code>track apply --dry-run</code></div>
    </div>
    <div class="backend-feature-cells">
      <div class="backend-feature-cell" data-status="full"><span>YouTrack</span><strong>Full</strong><em>JSON + cache</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Jira</span><strong>Full</strong><em>JSON + cache</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitHub</span><strong>Full</strong><em>JSON + cache</em></div>
      <div class="backend-feature-cell" data-status="full"><span>GitLab</span><strong>Full</strong><em>JSON + cache</em></div>
      <div class="backend-feature-cell" data-status="full"><span>Linear</span><strong>Full</strong><em>JSON + cache</em></div>
    </div>
  </article>
</section>

## YouTrack

<section class="backend-detail">
  <div class="backend-detail-main">
    <p>
      YouTrack is the most complete backend in <code>track</code>: issue CRUD,
      comments, links, tags, custom fields, field administration, projects, and
      Knowledge Base articles all use first-party YouTrack APIs.
    </p>
    <div class="backend-fact-grid">
      <div>
        <h3>Configure</h3>
        <p><code>YOUTRACK_URL</code>, <code>YOUTRACK_TOKEN</code>, optional <code>default_project</code>.</p>
      </div>
      <div>
        <h3>Identifiers</h3>
        <p>Issues use readable IDs such as <code>PROJ-123</code>. Articles use readable Knowledge Base IDs such as <code>KB-A-1</code>.</p>
      </div>
      <div>
        <h3>Search</h3>
        <p>Use YouTrack query syntax, for example <code>project: PROJ #Unresolved</code>.</p>
      </div>
      <div>
        <h3>Good fit</h3>
        <p>Automation that needs issue state, custom fields, tags, links, articles, and project metadata from one system.</p>
      </div>
    </div>
  </div>
  <aside class="backend-command-panel" aria-label="YouTrack examples">
    <pre><code>track -b yt i s "project: PROJ #Unresolved" --all
track -b yt wiki list -p PROJ --all
track -b yt field list</code></pre>
  </aside>
</section>

## Jira

<section class="backend-detail backend-detail-featured">
  <div class="backend-detail-main">
    <p>
      Jira support targets Jira Cloud and uses Confluence for article commands.
      It is especially useful when teams want tracker automation and wiki
      publishing from the same agent-friendly command surface.
    </p>
    <div class="backend-callout">
      <strong>Markdown wiki publishing:</strong> article content passed with
      <code>--body-file</code> can be authored as Markdown and converted to
      Confluence storage format when creating or updating pages. Agents can
      draft a runbook in the repo, review it as a normal file, then publish it
      without writing Confluence XML by hand.
    </div>
    <div class="backend-fact-grid">
      <div>
        <h3>Configure</h3>
        <p><code>JIRA_URL</code>, <code>JIRA_EMAIL</code>, and <code>JIRA_TOKEN</code>. Authentication uses Basic Auth with email and API token.</p>
      </div>
      <div>
        <h3>Identifiers</h3>
        <p>Issues use Jira keys such as <code>PROJ-123</code>. Confluence pages and spaces use numeric IDs.</p>
      </div>
      <div>
        <h3>Rich text</h3>
        <p>Issue descriptions and comments use Atlassian Document Format. Rich-text custom fields are surfaced as rendered plain text.</p>
      </div>
      <div>
        <h3>Fields and labels</h3>
        <p>System and custom fields are preserved in <code>custom_fields</code>. Jira labels map to common CLI tags.</p>
      </div>
    </div>
  </div>
  <aside class="backend-command-panel" aria-label="Jira examples">
    <pre><code>track -b j i s "project = PROJ AND resolution IS EMPTY" --all
track -b j wiki create -p 65957 -s "Rollback runbook" \
  --body-file ./runbook.md
track -b j wiki update 123456 --body-file ./runbook.md</code></pre>
  </aside>
</section>

### Jira quirks to know

<div class="backend-note-list">
  <p><strong>Confluence path:</strong> Jira article commands use Confluence at the same Atlassian domain with a <code>/wiki</code> path.</p>
  <p><strong>Components:</strong> surfaced as a <code>Components</code> multi-value custom field. Filter by area with JQL such as <code>component = "Rendering"</code>.</p>
  <p><strong>Project creation:</strong> generally requires administrator workflows in Jira; use the web interface for project setup.</p>
  <p><strong>Subtasks:</strong> create new subtasks with <code>--parent</code>, or link existing issues with <code>issue link -t subtask</code>.</p>
</div>

## GitHub

<section class="backend-detail">
  <div class="backend-detail-main">
    <p>
      GitHub support is repository-scoped and maps GitHub Issues into the common
      issue model. It filters pull requests out of issue listings so searches
      behave like issue-only workflows.
    </p>
    <div class="backend-fact-grid">
      <div>
        <h3>Configure</h3>
        <p><code>GITHUB_TOKEN</code>, <code>GITHUB_OWNER</code>, <code>GITHUB_REPO</code>, optional <code>GITHUB_API_URL</code>.</p>
      </div>
      <div>
        <h3>Identifiers</h3>
        <p>Use numeric issue numbers such as <code>42</code>. The configured owner/repo supplies the repository scope.</p>
      </div>
      <div>
        <h3>Labels</h3>
        <p>GitHub labels map to common CLI tags, including label colors.</p>
      </div>
      <div>
        <h3>Hierarchy</h3>
        <p>Sub-issues are supported through GitHub's sub-issues API with <code>--parent</code> or <code>issue link -t subtask/parent</code>.</p>
      </div>
    </div>
  </div>
  <aside class="backend-command-panel" aria-label="GitHub examples">
    <pre><code>track -b gh 42 --full
track -b gh i s "is:open label:bug" --all
track -b gh i link 42 43 -t subtask</code></pre>
  </aside>
</section>

### GitHub limits

<div class="backend-note-list">
  <p><strong>No issue deletion:</strong> GitHub does not support deleting issues through the Issues API; close them instead.</p>
  <p><strong>No general issue links:</strong> use comments such as <code>#42</code> for related references unless you are using sub-issues.</p>
  <p><strong>Wiki support:</strong> article commands use the repository wiki and Markdown pages. GitHub wiki pages do not support comments.</p>
  <p><strong>Rate limits:</strong> use authenticated requests for reliable automation.</p>
</div>

## GitLab

<section class="backend-detail">
  <div class="backend-detail-main">
    <p>
      GitLab support is project-scoped and uses GitLab REST API v4 for issue
      operations, with GraphQL used where parent-child work item hierarchy is
      required.
    </p>
    <div class="backend-fact-grid">
      <div>
        <h3>Configure</h3>
        <p><code>GITLAB_TOKEN</code>, <code>GITLAB_URL</code>, and <code>GITLAB_PROJECT_ID</code>. The URL should point at <code>/api/v4</code>.</p>
      </div>
      <div>
        <h3>Identifiers</h3>
        <p>Issue commands use the project IID, such as <code>#42</code>, not the global GitLab issue ID.</p>
      </div>
      <div>
        <h3>Labels and notes</h3>
        <p>Labels map to common CLI tags. GitLab comments are notes; system notes are filtered from comment lists.</p>
      </div>
      <div>
        <h3>Links</h3>
        <p>Supports <code>relates_to</code>, <code>blocks</code>, and <code>is_blocked_by</code>, plus parent-child hierarchy through GraphQL.</p>
      </div>
    </div>
  </div>
  <aside class="backend-command-panel" aria-label="GitLab examples">
    <pre><code>track -b gl 42 --full
track -b gl i s "state=opened" --all
track -b gl i link 42 43 -t depends</code></pre>
  </aside>
</section>

### GitLab limits

<div class="backend-note-list">
  <p><strong>No project creation:</strong> configure an existing project with <code>gitlab.project_id</code>.</p>
  <p><strong>Wiki support:</strong> article commands use GitLab project wiki pages. Wiki comments and moving wiki pages are not supported.</p>
  <p><strong>Project path IDs:</strong> numeric project IDs are simplest, but URL-encoded project paths can also be used where configured.</p>
</div>

## Linear

<section class="backend-detail">
  <div class="backend-detail-main">
    <p>
      Linear support is team-scoped. The CLI's <code>project</code> concept maps
      to a Linear team key, name, or ID; Linear's native Project is an issue
      association that can be set through a field or default config.
    </p>
    <div class="backend-fact-grid">
      <div>
        <h3>Configure</h3>
        <p><code>LINEAR_TOKEN</code>, <code>LINEAR_URL</code>, <code>LINEAR_DEFAULT_TEAM</code>, optional <code>LINEAR_DEFAULT_PROJECT</code>.</p>
      </div>
      <div>
        <h3>Identifiers</h3>
        <p>Use Linear issue IDs such as <code>ORE-123</code>. Team scope comes from <code>default_project</code> or <code>linear.default_team</code>.</p>
      </div>
      <div>
        <h3>Labels and projects</h3>
        <p>Labels map to tags. Unknown labels on create/update are rejected. Linear Project can be set with <code>--field "Project=Track CLI"</code>.</p>
      </div>
      <div>
        <h3>Links</h3>
        <p>Parent-child uses Linear parent IDs. Relation links support <code>related</code>, <code>blocks</code>, <code>duplicate</code>, and <code>similar</code>.</p>
      </div>
    </div>
  </div>
  <aside class="backend-command-panel" aria-label="Linear examples">
    <pre><code>track -b lin ORE-123 --full
track -b lin i s "#Open" --all
track -b lin i u ORE-123 --field "Project=Track CLI"</code></pre>
  </aside>
</section>

### Linear limits

<div class="backend-note-list">
  <p><strong>No team creation:</strong> configure an existing Linear team for CLI project scope.</p>
  <p><strong>No Knowledge Base:</strong> article and wiki commands are not available for Linear.</p>
  <p><strong>Project naming:</strong> Linear Project is not the same thing as the CLI project selector; use the Project field when you mean Linear's native project association.</p>
</div>
