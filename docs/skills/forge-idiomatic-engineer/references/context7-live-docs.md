# Context7 Live Docs Workflow

Use this whenever API behavior may have changed or is uncertain.

Typical triggers:
- Svelte 5 runes and evolving frontend APIs
- third-party SDK usage details
- Forge ecosystem integrations outside core repo docs
- version-specific framework behavior

## Step 1: Find Library ID

```bash
curl -s "https://context7.com/api/v2/libs/search?libraryName=LIBRARY_NAME&query=TOPIC" | jq '.results[0]'
```

Fields to inspect:
- `id`
- `title`
- `description`
- `totalSnippets`

## Step 2: Fetch Topic Documentation

```bash
curl -s "https://context7.com/api/v2/context?libraryId=LIBRARY_ID&query=TOPIC&type=txt"
```

Use `type=txt` for readability.

## Good Query Examples

### Svelte 5 runes
```bash
curl -s "https://context7.com/api/v2/libs/search?libraryName=svelte&query=runes" | jq '.results[0].id'

curl -s "https://context7.com/api/v2/context?libraryId=/sveltejs/svelte&query=%24state+%24derived+%24effect&type=txt"
```

### SQLx transaction patterns
```bash
curl -s "https://context7.com/api/v2/libs/search?libraryName=sqlx&query=transaction" | jq '.results[0].id'

curl -s "https://context7.com/api/v2/context?libraryId=/launchbadge/sqlx&query=postgres+transaction&type=txt"
```

### Axum middleware and tracing
```bash
curl -s "https://context7.com/api/v2/libs/search?libraryName=axum&query=middleware+tracing" | jq '.results[0].id'

curl -s "https://context7.com/api/v2/context?libraryId=/tokio-rs/axum&query=trace+middleware&type=txt"
```

## Integration Rule

When Context7 conflicts with memory, prefer Context7 + local repository reality.

Document in your solution:
- what was looked up
- which library ID was used
- what decision changed because of the lookup

## Reliability Tips

- URL-encode queries containing spaces (`+` or `%20`)
- If result[0] looks wrong, inspect `results[1..]`
- Keep query narrow and specific for better snippet relevance
