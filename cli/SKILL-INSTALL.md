# Toani CLI Skill Install Check

After installing/upgrading CLI skill, validate broker-only sandbox commands:

```bash
toani-vault --help
toani-vault sandbox --help
toani-vault sandbox request --operation-type http_request --params '{"method":"GET","url":"https://api.example.com/health"}'
toani-vault sandbox get-request <operationId>
```

If any output still references legacy session lifecycle commands, treat it as doc drift and refresh skill docs.
