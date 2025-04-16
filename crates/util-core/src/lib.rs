// SPDX-License-Identifier: MIT

pub fn is_env_var_set(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| v != "0" && v != "false")
}
