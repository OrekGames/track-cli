\n- Prefer using `str::eq_ignore_ascii_case` over `.to_lowercase() ==` for case-insensitive string comparisons to prevent unnecessary heap allocations.
