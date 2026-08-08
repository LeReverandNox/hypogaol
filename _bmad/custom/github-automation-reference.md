# GitHub Story-Cycle Automation Reference

Repo: LeReverandNox/hypogaol
GitHub Project: "Hypogaol", number 3, owner LeReverandNox
- projectId: PVT_kwHOAIY8w84BeI8Q
- Status field id: PVTSSF_lAHOAIY8w84BeI8QzhYlXTo
- Status options: Backlog=f75ad846, In Progress=47fc9ee4, In Review=07ee5c6d, Done=98236657

## Deriving from {{story_key}} (e.g. "1-2-user-authentication")
- epic_num = first segment before the first dash (e.g. "1")
- story_num = second segment (e.g. "2")
- story_id = "<epic_num>.<story_num>" (e.g. "1.2")
- slug = remainder after the second dash (e.g. "user-authentication")
- branch = "story/<story_id>-<slug>" (e.g. "story/1.2-user-authentication")
- epic label: 1 -> "epic:1-foundation", 2 -> "epic:2-key-lifecycle", 3 -> "epic:3-lifecycle-access", 4 -> "epic:4-advanced-operations-automation", 5 -> "epic:5-rebrand-to-hypogaol", 6 -> "epic:6-volume-resilience-filesystem-polish"
- Issue title convention: "Story <story_id>: <story title>"

## Moving a project item's Status
1. Find the item id: `gh project item-list 3 --owner LeReverandNox --format json`, match the item whose `content.number` equals the Issue number.
2. `gh api graphql -f query='mutation{updateProjectV2ItemFieldValue(input:{projectId:"PVT_kwHOAIY8w84BeI8Q",itemId:"<ITEM_ID>",fieldId:"PVTSSF_lAHOAIY8w84BeI8QzhYlXTo",value:{singleSelectOptionId:"<OPTION_ID>"}}){clientMutationId}}'`

## Manual-only transitions — never automate
- Merging a PR
- Moving an Issue to "Done"

## Failure handling
If any `gh`/`git` command in this automation fails, tell the user exactly what failed and continue — never block story creation, implementation, or review on GitHub automation.
