//! Deterministic repository facts for manifest drafting (no model involved).
use crate::manifest::schema::InputFieldSpec;
use indexmap::IndexMap;
use serde::Serialize;
use std::path::Path;

const SKIP: &[&str] = &[".git", ".venv", "venv", "node_modules", "__pycache__", "outputs", "target", "dist", ".mypy_cache", ".pytest_cache", ".idea", ".vscode"];
const README_MAX: usize = 7000;
pub const PROMPT_MAX: usize = 14000;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EntryCandidate {
    pub path: String,
    pub framework: String,
    /// Starts a server: web framework detected, or the CLI has a --port/--host flag.
    pub server: bool,
    pub inputs: IndexMap<String, InputFieldSpec>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Signals {
    pub has_venv: bool,
    pub uses_uv: bool,
    pub uses_pnpm: bool,
    pub uses_cargo: bool,
    pub gpu_hints: Vec<String>,
    pub web_hints: Vec<String>,
    pub uses_homemade_worker: bool,
    pub has_homemade_toml: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoFacts {
    pub root: String,
    pub readme: Option<(String, String)>,
    pub docs: Vec<(String, String)>,
    pub manifests: Vec<(String, String)>,
    pub tree: Vec<String>,
    pub entry_candidates: Vec<EntryCandidate>,
    /// pyproject `[project.scripts]` console entry points (name, target).
    pub console_scripts: Vec<(String, String)>,
    pub signals: Signals,
}

fn skip(name: &str) -> bool { SKIP.contains(&name) || name == ".DS_Store" }

fn head_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n).collect::<String>() + "\n…" }
}

fn read(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    if !m.is_file() || m.len() > 2_000_000 { return None; }
    std::fs::read(p).ok().map(|b| String::from_utf8_lossy(&b).to_string())
}

fn list_dir(root: &Path, rel: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root.join(rel)).map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        if skip(&n) { return None; }
        let dir = e.path().is_dir();
        Some(format!("{}{}{}", if rel.is_empty() { String::new() } else { format!("{rel}/") }, n, if dir { "/" } else { "" }))
    }).collect()).unwrap_or_default();
    v.sort();
    v
}

pub fn collect(root: &Path, res_dir: &Path) -> RepoFacts {
    let mut f = RepoFacts { root: root.to_string_lossy().to_string(), ..Default::default() };

    // tree
    let mut tree = list_dir(root, "");
    for sub in ["scripts", "src", "app"] {
        if root.join(sub).is_dir() { tree.extend(list_dir(root, sub)); }
    }
    tree.truncate(120);
    f.tree = tree;

    // readme
    let top: Vec<String> = list_dir(root, "").into_iter().filter(|p| !p.ends_with('/')).collect();
    let readme_name = top.iter().find(|n| n.to_lowercase().starts_with("readme") && n.to_lowercase().ends_with(".md"))
        .or_else(|| top.iter().find(|n| n.to_lowercase().starts_with("readme"))).cloned();
    if let Some(n) = &readme_name {
        if let Some(t) = read(&root.join(n)) { f.readme = Some((n.clone(), head_chars(&t, README_MAX))); }
    }

    // docs: other root *.md + docs/**/*.md
    let mut docs: Vec<String> = top.iter().filter(|n| n.to_lowercase().ends_with(".md") && Some(*n) != readme_name.as_ref()
        && !matches!(n.to_lowercase().as_str(), "license.md" | "changelog.md")).cloned().collect();
    if root.join("docs").is_dir() {
        for e in walkdir::WalkDir::new(root.join("docs")).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            if e.path().extension().map(|x| x == "md").unwrap_or(false) {
                if let Ok(r) = e.path().strip_prefix(root) { docs.push(r.to_string_lossy().to_string()); }
            }
        }
    }
    for d in docs.into_iter().take(8) {
        if let Some(t) = read(&root.join(&d)) {
            let title = t.lines().find(|l| l.starts_with('#')).map(|l| l.trim_start_matches('#').trim().to_string()).unwrap_or_default();
            let body = head_chars(t.trim(), 400);
            f.docs.push((d, if title.is_empty() { body } else { format!("{title}: {body}") }));
        }
    }

    // manifests
    let mut deps_text = String::new();
    if let Some(t) = read(&root.join("pyproject.toml")) {
        deps_text.push_str(&t);
        let mut out = String::new();
        if let Ok(v) = t.parse::<toml::Table>() {
            if let Some(p) = v.get("project").and_then(|p| p.as_table()) {
                for k in ["name", "description", "requires-python"] {
                    if let Some(x) = p.get(k).and_then(|x| x.as_str()) { out.push_str(&format!("{k} = {x}\n")); }
                }
                if let Some(s) = p.get("scripts").and_then(|s| s.as_table()) {
                    for (k, x) in s {
                        out.push_str(&format!("script {k} = {}\n", x.as_str().unwrap_or("")));
                        f.console_scripts.push((k.clone(), x.as_str().unwrap_or("").to_string()));
                    }
                }
                if let Some(d) = p.get("dependencies").and_then(|d| d.as_array()) {
                    let ds: Vec<&str> = d.iter().filter_map(|x| x.as_str()).take(25).collect();
                    out.push_str(&format!("dependencies = {}\n", ds.join(", ")));
                }
            }
        }
        if out.is_empty() { out = head_chars(&t, 800); }
        f.manifests.push(("pyproject.toml".into(), out));
    }
    if let Some(t) = read(&root.join("package.json")) {
        deps_text.push_str(&t);
        let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
        let mut out = format!("name = {}\n", v.get("name").and_then(|x| x.as_str()).unwrap_or(""));
        if let Some(s) = v.get("scripts").and_then(|s| s.as_object()) {
            for (k, x) in s { out.push_str(&format!("script {k} = {}\n", x.as_str().unwrap_or(""))); }
        }
        f.manifests.push(("package.json".into(), out));
    }
    for n in top.iter().filter(|n| n.starts_with("requirements") && n.ends_with(".txt")) {
        if let Some(t) = read(&root.join(n)) {
            deps_text.push_str(&t);
            f.manifests.push((n.clone(), t.lines().take(40).collect::<Vec<_>>().join("\n")));
        }
    }
    if let Some(t) = read(&root.join("Cargo.toml")) { f.manifests.push(("Cargo.toml".into(), head_chars(&t, 600))); }
    if let Some(t) = read(&root.join("Makefile")) {
        let targets: Vec<&str> = t.lines().filter(|l| !l.starts_with(['\t', ' ', '#', '.']) && l.contains(':') && !l.contains(":="))
            .filter_map(|l| l.split(':').next()).take(30).collect();
        f.manifests.push(("Makefile".into(), format!("targets: {}", targets.join(" "))));
    }

    // entry candidates
    let mut pys: Vec<String> = top.iter().filter(|n| n.ends_with(".py")).cloned().collect();
    if root.join("scripts").is_dir() { pys.extend(list_dir(root, "scripts").into_iter().filter(|n| n.ends_with(".py"))); }
    let mut all_src = String::new();
    for rel in pys.iter().take(40) {
        let Some(src) = read(&root.join(rel)) else { continue };
        let fw = detect_framework(&src);
        all_src.push_str(&src);
        if fw.is_none() && !src.contains("__main__") { continue; }
        if f.entry_candidates.len() >= 8 { continue; }
        let inputs = crate::manifest::infer::run_inspector(&root.join(rel), res_dir).unwrap_or_default();
        let web_fw = matches!(fw, Some("gradio" | "streamlit" | "fastapi" | "flask" | "http.server"));
        let has_port = inputs.values().any(|i| matches!(i.cli_flag.as_deref(), Some("--port") | Some("--host")));
        f.entry_candidates.push(EntryCandidate { path: rel.clone(), framework: fw.unwrap_or("script").into(), server: web_fw || has_port, inputs, doc: module_doc(&src) });
    }
    // Servers first, then real CLIs, then plain scripts — the first candidate is what the fallback uses.
    let rank = |e: &EntryCandidate| if e.server { 0 } else if matches!(e.framework.as_str(), "argparse" | "typer" | "click") { 1 } else { 2 };
    f.entry_candidates.sort_by_key(rank);
    // Package sources (dirs with __init__.py) feed the gpu/web signals even though they are not entries.
    for d in top.iter().filter(|n| n.ends_with('/')).map(|n| n.trim_end_matches('/').to_string()) {
        if !root.join(&d).join("__init__.py").is_file() { continue; }
        for n in list_dir(root, &d).into_iter().filter(|n| n.ends_with(".py")).take(40) {
            if let Some(src) = read(&root.join(&n)) { all_src.push_str(&src); }
        }
    }

    // signals
    let s = &mut f.signals;
    s.has_venv = root.join(".venv").is_dir() || root.join("venv").is_dir();
    s.uses_uv = root.join("uv.lock").exists() || root.join("pyproject.toml").exists();
    s.uses_pnpm = root.join("pnpm-lock.yaml").exists();
    s.uses_cargo = root.join("Cargo.toml").exists();
    s.has_homemade_toml = root.join("homemade.toml").exists();
    s.uses_homemade_worker = all_src.contains("homemade_worker");
    let hay = format!("{deps_text}\n{all_src}").to_lowercase();
    for k in ["torch", "mlx", "cuda", "tensorflow", "jax", "diffusers", "transformers"] {
        if hay.contains(k) { s.gpu_hints.push(k.into()); }
    }
    for k in ["gradio", "streamlit", "flask", "fastapi", "uvicorn", "http.server"] {
        if hay.contains(k) { s.web_hints.push(k.into()); }
    }
    f
}

fn detect_framework(src: &str) -> Option<&'static str> {
    for (k, n) in [("gradio", "gradio"), ("streamlit", "streamlit"), ("fastapi", "fastapi"), ("flask", "flask"), ("http.server", "http.server"),
                   ("import typer", "typer"), ("import click", "click"), ("argparse", "argparse")] {
        if src.contains(k) { return Some(n); }
    }
    None
}

fn module_doc(src: &str) -> Option<String> {
    let t = src.trim_start();
    let t = if t.starts_with("#!") { t.split_once('\n').map(|x| x.1).unwrap_or("").trim_start() } else { t };
    for q in ["\"\"\"", "'''"] {
        if let Some(rest) = t.strip_prefix(q) {
            return rest.split_once(q).map(|(d, _)| head_chars(d.trim(), 300));
        }
    }
    None
}

impl RepoFacts {
    pub fn to_prompt_text(&self) -> String { self.to_prompt_text_max(PROMPT_MAX) }

    /// Rendered facts, at most `max` chars; the README is truncated first, then everything else is cut.
    pub fn to_prompt_text_max(&self, max: usize) -> String {
        let rest = self.render_without_readme();
        let Some((name, body)) = &self.readme else { return head_chars(&rest, max) };
        let header = format!("## {name}\n");
        let budget = max.saturating_sub(rest.chars().count() + header.chars().count() + 4);
        if budget < 200 { return head_chars(&format!("{rest}\n{header}{}", head_chars(body, 200)), max); }
        format!("{header}{}\n\n{rest}", head_chars(body, budget))
    }

    fn render_without_readme(&self) -> String {
        let mut o = String::new();
        o.push_str(&format!("## Tree ({})\n{}\n", self.root, self.tree.join("\n")));
        if !self.manifests.is_empty() {
            o.push_str("\n## Manifests\n");
            for (n, t) in &self.manifests { o.push_str(&format!("### {n}\n{t}\n")); }
        }
        if !self.entry_candidates.is_empty() {
            o.push_str("\n## Entry candidates\n");
            for e in &self.entry_candidates {
                o.push_str(&format!("- {} ({}{})", e.path, e.framework, if e.server { ", server: starts an HTTP server" } else { "" }));
                if let Some(d) = &e.doc { o.push_str(&format!(" — {}", d.replace('\n', " "))); }
                o.push('\n');
                for (k, v) in &e.inputs {
                    let how = v.cli_flag.clone().or(v.positional_index.map(|i| format!("positional {i}"))).unwrap_or_default();
                    o.push_str(&format!("  - {k}: {} {how}{}{}\n", v.field_type,
                        v.default.as_ref().map(|d| format!(" default={d}")).unwrap_or_default(),
                        v.description.as_ref().or(v.label.as_ref()).map(|d| format!(" \"{d}\"")).unwrap_or_default()));
                }
            }
        }
        if !self.console_scripts.is_empty() {
            o.push_str("\n## Console scripts (pyproject [project.scripts])\n");
            for (k, v) in &self.console_scripts { o.push_str(&format!("- {k} -> {v}\n")); }
        }
        if !self.docs.is_empty() {
            o.push_str("\n## Docs\n");
            for (n, t) in &self.docs { o.push_str(&format!("- {n}: {}\n", t.replace('\n', " "))); }
        }
        let s = &self.signals;
        o.push_str(&format!("\n## Signals\nhas_venv={} uses_uv={} uses_pnpm={} uses_cargo={} uses_homemade_worker={} has_homemade_toml={} gpu_hints=[{}] web_hints=[{}]\n",
            s.has_venv, s.uses_uv, s.uses_pnpm, s.uses_cargo, s.uses_homemade_worker, s.has_homemade_toml, s.gpu_hints.join(","), s.web_hints.join(",")));
        o
    }

    /// One-line summary for the UI.
    pub fn summary(&self) -> String {
        format!("README: {} · docs: {} · manifests: [{}] · entries: [{}] · gpu: [{}] · web: [{}]",
            self.readme.as_ref().map(|r| r.0.as_str()).unwrap_or("none"), self.docs.len(),
            self.manifests.iter().map(|m| m.0.as_str()).collect::<Vec<_>>().join(", "),
            self.entry_candidates.iter().map(|e| format!("{} ({})", e.path, e.framework)).collect::<Vec<_>>().join(", "),
            self.signals.gpu_hints.join(","), self.signals.web_hints.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res() -> std::path::PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("resources") }

    #[test]
    fn collects_hello_cli() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/hello-cli");
        let f = collect(&root, &res());
        assert!(f.tree.iter().any(|p| p == "main.py"));
        assert!(!f.tree.iter().any(|p| p.starts_with("outputs")));
        assert!(f.signals.has_homemade_toml);
        assert!(f.entry_candidates.iter().any(|e| e.path == "main.py"));
    }

    #[test]
    fn collects_fixture_and_truncates_readme_first() {
        let dir = std::env::temp_dir().join(format!("hm-facts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("README.md"), format!("# Tool\nDoes things.\n{}", "lorem ipsum ".repeat(2000))).unwrap();
        std::fs::write(dir.join("docs/usage.md"), "# Usage\nRun it.").unwrap();
        std::fs::write(dir.join("requirements.txt"), "torch\ngradio\n").unwrap();
        std::fs::write(dir.join("app.py"), "\"\"\"Web demo.\"\"\"\nimport argparse\np = argparse.ArgumentParser()\np.add_argument('--port', type=int, default=7860)\nif __name__ == '__main__':\n    p.parse_args()\n").unwrap();
        let f = collect(&dir, &res());
        assert_eq!(f.readme.as_ref().unwrap().0, "README.md");
        assert!(f.readme.as_ref().unwrap().1.chars().count() <= README_MAX + 2);
        assert!(f.docs.iter().any(|d| d.0 == "docs/usage.md" && d.1.starts_with("Usage")));
        assert!(f.signals.gpu_hints.contains(&"torch".to_string()));
        assert!(f.signals.web_hints.contains(&"gradio".to_string()));
        assert!(!f.tree.iter().any(|p| p.contains("node_modules")));
        let e = f.entry_candidates.iter().find(|e| e.path == "app.py").unwrap();
        assert_eq!(e.doc.as_deref(), Some("Web demo."));
        let full = f.to_prompt_text();
        assert!(full.chars().count() <= PROMPT_MAX);
        let small = f.to_prompt_text_max(3000);
        assert!(small.chars().count() <= 3000);
        assert!(small.contains("## Signals") && small.contains("app.py"), "non-README facts survive truncation");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
