use crate::payload::PayloadManifest;

const TEMPLATE: &str = include_str!("../../dist/init.template.sh");

/// Render the busybox start-stop-daemon init script for a payload.
/// Uses simple {{key}} token replacement — no external templating crate.
pub fn render(manifest: &PayloadManifest) -> String {
    let pre_start = if manifest.pre_start.is_empty() {
        String::new()
    } else {
        // Each pre_start command gets its own indented line before start-stop-daemon
        manifest
            .pre_start
            .iter()
            .map(|cmd| format!("    {cmd}\n"))
            .collect::<String>()
    };

    TEMPLATE
        .replace("{{name}}", &manifest.name)
        .replace("{{data_dir}}", &manifest.data_dir)
        .replace("{{binary_name}}", &manifest.binary_name)
        .replace("{{pidfile}}", &manifest.pidfile)
        .replace("{{log_file}}", &manifest.log_file)
        .replace("{{args}}", &manifest.args)
        .replace("{{pre_start}}", &pre_start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::PayloadManifest;

    fn sample_manifest() -> PayloadManifest {
        PayloadManifest {
            name: "my-tool".into(),
            version: "1.0.0".into(),
            data_dir: "/data/my-tool".into(),
            binary_name: "my-tool".into(),
            args: "--port 8081".into(),
            log_file: "/data/my-tool/my-tool.log".into(),
            pidfile: "/tmp/my-tool.pid".into(),
            ports: vec![8081],
            pre_start: vec![],
        }
    }

    #[test]
    fn render_contains_binary_path() {
        let script = render(&sample_manifest());
        assert!(script.contains("/data/my-tool/my-tool"));
    }

    #[test]
    fn render_contains_args() {
        let script = render(&sample_manifest());
        assert!(script.contains("--port 8081"));
    }

    #[test]
    fn render_contains_all_cases() {
        let script = render(&sample_manifest());
        for case in ["start", "stop", "restart", "status"] {
            assert!(script.contains(case), "missing case: {case}");
        }
    }

    #[test]
    fn render_pre_start_injected() {
        let mut m = sample_manifest();
        m.pre_start = vec!["mkdir -p /data/my-tool/state".into()];
        let script = render(&m);
        assert!(script.contains("mkdir -p /data/my-tool/state"));
    }
}
