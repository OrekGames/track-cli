sed -i 's/if let Some(mut stream) = listener.incoming().flatten().next() {/for stream in listener.incoming().flatten() {/' crates/track/tests/cli_tests.rs
cargo test -p track --test cli_tests -- test_config_file_is_used_for_defaults
