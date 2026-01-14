# AI-Friendly Diagnostic Data Export Guide

## Overview
This guide provides recommendations for exporting diagnostic data from your CLI tool in formats that are optimized for AI tool processing (like Claude.ai). The goal is to make it easy for users to get help by generating data dumps they can upload alongside diagnostic prompts.

## Format Recommendation: JSON

**Primary recommendation: Use JSON for your diagnostic exports**

### Why JSON Works Best

#### Advantages:
- **Native support**: Most AI tools can directly parse and understand JSON structure without writing code
- **Hierarchical data**: Perfect for representing related database records (foreign keys, nested relationships)
- **Self-documenting**: Field names are included, making it clear what each value represents
- **Flexible**: Can represent complex types (arrays, nested objects) that CSV struggles with

#### When CSV is Acceptable:
CSV can work fine for specific scenarios:
- **Single-table dumps**: When you're only exporting one table with simple data
- **Visual scanning**: CSV is slightly easier to scan visually for humans
- **Simplicity**: No nested structures needed

#### CSV Limitations:
- Loses relational structure (requires multiple files for related tables)
- Handles simple tabular data only
- No support for nested or complex types
- AI tools can view CSV directly, but relational context is lost

## Recommended Data Structures

### For Single-Table Dumps
Either JSON or CSV works fine:

**JSON example:**
```json
{
  "users": [
    {
      "id": 1,
      "username": "john_doe",
      "created_at": "2025-01-01T10:00:00Z",
      "status": "active"
    },
    {
      "id": 2,
      "username": "jane_smith",
      "created_at": "2025-01-02T14:30:00Z",
      "status": "active"
    }
  ]
}
```

### For Multi-Table or Relational Data
Definitely use JSON to preserve relationships:

```json
{
  "diagnostic_info": {
    "timestamp": "2025-01-13T10:30:00Z",
    "version": "1.2.3",
    "user_input": "...",
    "error_type": "database_connection"
  },
  "users": [
    {
      "id": 1,
      "username": "john_doe",
      "created_at": "2025-01-01T10:00:00Z"
    }
  ],
  "sessions": [
    {
      "id": 101,
      "user_id": 1,
      "started_at": "2025-01-13T09:00:00Z",
      "status": "active"
    }
  ],
  "error_logs": [
    {
      "id": 501,
      "session_id": 101,
      "timestamp": "2025-01-13T10:30:00Z",
      "message": "Connection timeout",
      "stack_trace": "..."
    }
  ]
}
```

### For Very Large Datasets

When dealing with datasets that might exceed context limits:

**Option 1: JSON Lines (`.jsonl`)**
One JSON object per line, easier to truncate/sample:
```jsonl
{"id": 1, "username": "john_doe", "created_at": "2025-01-01T10:00:00Z"}
{"id": 2, "username": "jane_smith", "created_at": "2025-01-02T14:30:00Z"}
{"id": 3, "username": "bob_jones", "created_at": "2025-01-03T11:15:00Z"}
```

**Option 2: Include only recent/relevant records**
- Filter to last N days/records
- Sort by relevance (most recent errors, affected users, etc.)
- Document filtering criteria clearly

## Best Practices

### 1. Include Metadata in the Export

Always include context about the data itself:

```json
{
  "_metadata": {
    "generated_at": "2025-01-13T10:30:00Z",
    "cli_version": "1.2.3",
    "db_version": "schema_v5",
    "total_record_count": 150,
    "filter_applied": "last 30 days",
    "export_reason": "diagnostic_report",
    "file_size_bytes": 45230
  },
  "data": {
    "users": [...],
    "sessions": [...],
    "error_logs": [...]
  }
}
```

### 2. Create a Guided Prompt Template

Help users structure their diagnostic request effectively:

```markdown
I'm troubleshooting [ISSUE_DESCRIPTION]. Attached is diagnostic data from my system.

**Key Details:**
- Software version: [VERSION]
- Error occurred at: [TIMESTAMP]
- User action that triggered issue: [ACTION]
- Error message: [ERROR_MESSAGE]

**Attached Data File:**
The JSON file (`diagnostic_export_[TIMESTAMP].json`) contains:
- `users` table: User accounts and authentication data
- `sessions` table: Active and recent user sessions
- `error_logs` table: Recent error entries related to this issue
- `_metadata`: Export context and filtering information

**Question:**
What might be causing this issue, and what steps should I take to resolve it?
```

### 3. Size Management

**In your CLI tool:**
- Calculate estimated export size before generating
- Warn users if file will exceed 2MB
- Offer filtering options:
  ```
  Warning: Full export would be 5.2MB (may exceed AI tool limits)
  
  Options:
  1. Export last 7 days only (estimated 800KB)
  2. Export last 30 days only (estimated 2.1MB)
  3. Export specific tables only
  4. Continue with full export
  
  Choose an option [1-4]:
  ```

### 4. File Naming Convention

Use descriptive, timestamped filenames:
```
diagnostic_export_2025-01-13_103045.json
error_logs_2025-01-13_103045.json
full_db_dump_2025-01-13_103045.jsonl
```

### 5. Show File Paths Clearly

After generation, display clear instructions:
```
✓ Diagnostic export generated successfully!

File location: /home/user/.myapp/exports/diagnostic_export_2025-01-13_103045.json
File size: 1.2 MB

Next steps:
1. Visit https://claude.ai
2. Start a new conversation
3. Click the attachment button and upload the file above
4. Copy and paste the prompt below:

[GENERATED PROMPT TEMPLATE HERE]
```

## Implementation Checklist

- [ ] Choose primary format (JSON recommended for most cases)
- [ ] Design data structure with metadata section
- [ ] Implement filtering for large datasets
- [ ] Add file size estimation and warnings
- [ ] Create prompt template with placeholders
- [ ] Show clear file paths after export
- [ ] Include schema version in metadata
- [ ] Document what each table/section contains
- [ ] Test with sample exports to verify AI tool compatibility
- [ ] Consider privacy: allow users to redact sensitive data

## Privacy Considerations

**Important:** Before exporting, consider:
- Allow users to preview what will be exported
- Provide option to redact sensitive fields (passwords, API keys, PII)
- Clearly document what data is included
- Consider a `--sanitize` flag to automatically strip sensitive data

Example sanitization:
```json
{
  "users": [
    {
      "id": 1,
      "username": "john_doe",
      "email": "j***@example.com",  // redacted
      "password_hash": "[REDACTED]",
      "api_key": "[REDACTED]"
    }
  ]
}
```

## Example CLI Workflow

```bash
# User encounters an error
$ myapp run-command
Error: Database connection failed

# User generates diagnostic report
$ myapp diagnose --export
Analyzing system state...
Collecting diagnostic data...

Export options:
1. Quick export (last 24 hours, ~500KB)
2. Standard export (last 7 days, ~2MB)
3. Full export (all data, ~8MB - may be too large)
4. Custom date range

Choose option [1-4]: 2

Generating export...
✓ Export complete!

File: /home/user/.myapp/diagnostics/export_2025-01-13_103045.json
Size: 1.8 MB

Ready to get AI assistance:
1. Visit https://claude.ai
2. Upload the file above
3. Use this prompt:

────────────────────────────────────────
I'm troubleshooting a database connection error in myapp v1.2.3.

Error occurred at: 2025-01-13 10:30:45 UTC
Error message: "Database connection failed"
User action: Running 'run-command'

Attached diagnostic export contains:
- Recent error logs (last 7 days)
- Active user sessions
- Database connection attempts
- System configuration

What might be causing this issue?
────────────────────────────────────────

Prompt copied to clipboard!
```

## Testing Your Implementation

Before releasing, test that:
1. Generated files are valid JSON (use `jq` or similar to validate)
2. File sizes are reasonable for typical use cases
3. AI tools can successfully parse and understand the data
4. Sensitive information is properly redacted
5. Metadata accurately describes the export
6. File paths are displayed correctly on different platforms (Windows/Mac/Linux)

## Additional Resources

- JSON validation: https://jsonlint.com
- JSON Lines format: https://jsonlines.org
- Claude.ai file upload limits: Check current documentation
- Privacy best practices: Consider GDPR/CCPA implications for user data
