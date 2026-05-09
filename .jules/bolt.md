Bolt Journal

- When normalizing maps and hashsets to be checked later on, one cannot avoid making memory allocations if string casing differ between input and expected output (i.e. replacing HashSet items to not allocate `.to_string()` without changing downstream iteration from `.contains()` to `iter().any(|r| r.eq_ignore_ascii_case())` changes algorithmic complexity). Therefore replacing `to_lowercase()` to `eq_ignore_ascii_case()` inside an iterator or if statement is a better idea instead.
