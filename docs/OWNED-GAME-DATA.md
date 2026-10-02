# Owned Skate 3 game data

This project does not contain and should not distribute Skate 3 retail assets.

The working precedent in the MW2 mashup is:

1. the user provides an owned Xbox 360 Skate 3 extraction,
2. `default.xex` is selected with the original `data/` folder beside it,
3. a local converter extracts only the data needed by the reconstructed runtime,
4. generated data remains outside version control.

Expected local layout may look like:

```text
retail-data/
  Skate 3/
    default.xex
    data/
skate-data/
  ...
```

Both folders are ignored by git.

Do not commit:

- `*.xex`,
- disc images,
- extracted retail archives,
- converted character/animation/physics assets.

The first prototype should accept a local path via configuration/environment and fail with a clear message when private data has not been prepared.
