# Knowledge base articles

Read when working with articles: YouTrack knowledge base, Confluence, or a GitHub/GitLab wiki.

## Article Operations (Knowledge Base)

| Operation | YouTrack | Jira/Confluence |
|-----------|----------|-----------------|
| Get | `track a g KB-A-1` | `track -b j a g 123456` |
| List | `track a ls --project PROJ` | `track -b j a ls --project 65957` |
| Search | `track a s "query"` | `track -b j a s "query"` |
| Create | `track a new -p PROJ -s "Title" -c "# Content"` | `track -b j a new -p 65957 -s "Title" -c "# Content"` |
| Update | `track a u KB-A-1 --summary "New"` | `track -b j a u 123456 --summary "New"` |
| Delete | `track a del KB-A-1` | `track -b j a del 123456` |
| Tree / Move | `track a tree KB-A-1` / `track a move KB-A-1 --parent KB-A-2` | same |
| Comments | `track a comments KB-A-1` | `track -b j a comments 123456` |
| Add comment | `track a cmt KB-A-1 -m "Text"` | `track -b j a cmt 123456 -m "Text"` |

**Notes**:
- Content is passed with `-c/--content "# Markdown"` or `--body-file page.md` (mutually exclusive); `--parent <ID>` nests; `a move` without `--parent` moves to root (Confluence does not support `a move` at all).
- **GitHub/GitLab support articles via the repo wiki** (`track -b gh a ls`, etc.) — except article comments, which they reject. Linear has no knowledge base.
- Confluence uses numeric page/space IDs; YouTrack uses readable IDs (e.g., `KB-A-1`).
