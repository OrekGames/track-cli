import re

with open('crates/track/src/cache.rs', 'r') as f:
    content = f.read()

# Fix the ignored Result
content = content.replace("    let _ = builder.create(path); // Ignore if exists", "    builder.create(path)?;")

with open('crates/track/src/cache.rs', 'w') as f:
    f.write(content)
