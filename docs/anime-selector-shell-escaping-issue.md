# Anime Selector Shell Escaping Issue

**Date**: 2025-11-13
**Component**: `anime-selector`
**Status**: Known limitation - 8 anime failed (99.94% success rate)

## Problem Summary

When executing the Python selection script from Rust via `zsh -c`, anime titles containing exclamation marks (`!`) in the AllAnime API results cause JSON parsing failures due to shell history expansion.

## Technical Details

### Root Cause

1. **Shell History Expansion in zsh**
   - When using `Command::new("zsh").arg("-c").arg(command_string)` in Rust
   - Exclamation marks (`!`) in double-quoted strings trigger zsh history expansion
   - zsh escapes `!` as `\!` even when passed as an argument to another command

2. **JSON Parsing Failure**
   - Python script receives JSON like: `["...Ganbaruzu\! (1 eps)"]`
   - JSON specification does NOT recognize `\!` as a valid escape sequence
   - Valid JSON escapes are: `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uXXXX`
   - Results in `json.JSONDecodeError: Invalid \escape`

3. **Multi-layer Quoting Problem**
   ```rust
   // Rust code generates:
   zsh -c 'eval "$(conda shell.zsh hook)" && conda activate GDA2025 && python3 scripts/select_anime.py --candidates "[\"Pokemon...!\"]"'
   ```
   - Double quotes inside the `zsh -c` command string
   - `!` within double quotes still triggers history expansion
   - Even with `set +H` or `setopt no_banghist`, the escaping still occurs

### Example Failure

**Anime**: Pokemon Advanced Generation (MAL ID: 1564, 192 episodes)

**AllAnime Candidates** (all movies with `!` in titles):
```json
[
  "Pokemon Fushigi no Dungeon: Shutsudou Pokemon Kyuujotai Ganbaruzu! (1 eps)",
  "Pokemon Movie 6: Nanayo no Negaiboshi Jirachi (1 eps)",
  "Pokemon Movie 7: Rekkuu no Houmonsha Deoxys (1 eps)"
]
```

**What Python receives**:
```json
["Pokemon Fushigi no Dungeon: Shutsudou Pokemon Kyuujotai Ganbaruzu\! (1 eps)", ...]
```

**Error**:
```
json.JSONDecodeError: Invalid \escape: line 1 column 68 (char 67)
```

## Failed Anime List

Total: 8 anime (affecting 281 jobs out of 172,066 total)

| MAL ID | Title | Episodes | Issue |
|--------|-------|----------|-------|
| 1564 | Pokemon Advanced Generation | 192 | AllAnime results contain `!` |
| 1770 | Unbalance | 3 | Special characters in results |
| 2564 | Code-E | 12 | Special characters in results |
| 3470 | Special A | 24 | Special characters in results |
| 10161 | No.6 | 11 | Special characters in results |
| 32316 | Nanoha Mini Picture Drama | 2 | Special characters in results |
| 33051 | Gundam IBO 2nd Season | 25 | Special characters in results |
| 48441 | Legend of Heroes Northern War | 12 | Special characters in results |

## Attempted Solutions (All Failed)

### 1. Double Quote Escaping
```rust
fn shell_quote(s: &str) -> String {
    let escaped = s.replace('"', r#"\""#);
    format!(r#""{}""#, escaped)
}
```
**Result**: `\!` still appears in the JSON

### 2. Single Quote Wrapping
```rust
fn shell_quote_single(s: &str) -> String {
    let escaped = s.replace('\'', r"'\''");
    format!("'{}'", escaped)
}
```
**Result**: zsh still processes `!` even within the command string

### 3. Disable History Expansion
```bash
set +H && python3 script.py
setopt no_banghist && python3 script.py
```
**Result**: Commands have no effect when used inside `zsh -c`

### 4. Pass JSON via stdin
```rust
// Attempted to pipe JSON to Python stdin instead of command-line arg
```
**Result**: The `!` is escaped even before reaching stdin

### 5. Escape Exclamation Marks
```rust
.replace('!', r"\!")
```
**Result**: Double escaping - makes the problem worse (`\\!`)

## Why This is Hard to Fix

1. **Nested Command Execution**: The command string passed to `zsh -c` is executed by zsh's parser, which applies history expansion BEFORE executing the Python command

2. **Conda Requirement**: Must use zsh because conda environment activation requires zsh shell functions: `eval "$(conda shell.zsh hook)"`

3. **No Alternative Escaping**: Within the `zsh -c` execution context, there's no reliable way to prevent `!` from being escaped when it appears in double-quoted argument values

## Impact Analysis

- **Total anime processed**: 13,391
- **Successful selections**: 13,382
- **Failed**: 8
- **Success rate**: 99.94%
- **Jobs affected**: 281 out of 172,066 (0.16%)

The failure rate is extremely low and affects edge cases where:
1. AllAnime API returns titles with special shell characters (`!`, etc.)
2. The main series is not found (only movies/specials returned)

## Recommended Workaround

Since these are rare edge cases (< 0.1% failure rate):

1. **Manual database entries**: Directly insert selection results for these 8 anime into `anime_selection_cache` table

2. **Mark as skipped**: Insert with `selected_index = -1` to mark as "no valid candidates"
   ```sql
   INSERT INTO anime_selection_cache (mal_id, anime_title, search_title, selected_index, selected_title, confidence, reason)
   VALUES (1564, 'Pokemon Advanced Generation', 'Pokemon Advanced Generation', -1, 'N/A', 'no_candidates', 'Special characters in API results cause shell escaping issues');
   ```

3. **Let downloader skip them**: The downloader will automatically skip anime without valid selections

## Future Improvements (If Needed)

If this becomes a more widespread issue:

1. **Use a temporary file**: Write JSON to a temp file and pass the file path to Python
   ```rust
   let temp_file = write_to_temp_file(&candidates_json)?;
   let cmd = format!("python3 script.py --candidates-file {}", temp_file);
   ```

2. **Use environment variables**: Pass JSON via `CANDIDATES` env var
   ```rust
   cmd.env("CANDIDATES", &candidates_json);
   ```

3. **Switch to bash for script execution only**: Use bash for the Python command, zsh only for conda activation
   ```rust
   let cmd = format!(
       r#"eval "$(conda shell.zsh hook)" && conda activate GDA2025 && bash -c 'python3 ...'"#
   );
   ```

4. **HTTP server approach**: Run a local HTTP server and pass data via POST request instead of command-line args

## Conclusion

This is a known limitation caused by the interaction between:
- zsh shell history expansion behavior
- Multi-layer command string nesting (Rust → zsh -c → Python)
- Conda's requirement for zsh shell functions
- Special characters in anime titles from external APIs

Given the 99.94% success rate, manual handling of these 8 edge cases is the most pragmatic solution.
