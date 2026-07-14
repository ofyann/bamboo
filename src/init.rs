use crate::cli::InitArgs;
use crate::config;
use crate::error::{BambooError, Result};
use std::path::Path;

pub fn run(args: InitArgs) -> Result<()> {
    let path = Path::new(&args.output);

    if path.exists() && !args.force {
        return Err(BambooError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "配置文件 {} 已存在，请加 --force 覆盖",
                path.display()
            ),
        )));
    }

    std::fs::write(path, config::default_template())?;
    println!("已生成配置文件: {}", path.display());
    Ok(())
}
