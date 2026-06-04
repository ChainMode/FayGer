//! `fayger` 可执行入口（M9）。
//!
//! 极薄壳：收集 argv（去掉程序名）→ [`fayger_cli::run_cli`] → 把错误写到 stderr →
//! 用 [`fayger_cli::exit_code_of`] 映射退出码。全部逻辑在 `fayger_cli` 库里，便于测试。

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdout = std::io::stdout().lock();
    let result = fayger_cli::run_cli(&args, &mut stdout);
    if let Err(e) = &result {
        eprintln!("{e}");
    }
    std::process::exit(fayger_cli::exit_code_of(&result).as_i32());
}
