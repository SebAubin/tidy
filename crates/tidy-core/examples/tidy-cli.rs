//! `tidy-cli scan <folder> <snapshot.json>` scans and writes a snapshot;
//! `tidy-cli mcp <snapshot.json>` serves it over MCP. For trying the engine without the window.
use std::sync::Arc;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("scan") => {
            let t = tidy_core::scan(std::path::Path::new(&a[2]), Arc::new(tidy_core::Progress::default())).unwrap();
            let home = Some(tidy_core::home_dir());
            let sug = tidy_core::suggestions(&t, home.as_deref());
            println!("{} in {} files, {} ms, {} denied, {} suggestions", tidy_core::format::bytes(t.total()), t.node(0).files, t.elapsed_ms, t.denied, sug.len());
            for s in sug.iter().take(10) { println!("  {:>10}  {}  {}", tidy_core::format::bytes(s.size), s.title, s.path); }
            tidy_core::snapshot::Snapshot::build(&t, &sug, &[]).write(std::path::Path::new(&a[3])).unwrap();
        }
        Some("dev") => {
            let root = std::path::PathBuf::from(&a[2]);
            let t = tidy_core::scan(&root, Arc::new(tidy_core::Progress::default())).unwrap();
            let home = tidy_core::home_dir();
            let (mut f, w) = tidy_core::dev::from_tree(&t, &home);
            let started = std::time::Instant::now();
            let sys = tidy_core::dev::from_system(&w, &mut f, None, &home);
            eprintln!("system checks: {} ms, {} worktrees", started.elapsed().as_millis(), w.len());
            f.extend(sys);
            for cat in tidy_core::dev::Category::ALL {
                let items: Vec<_> = f.iter().filter(|x| x.category == cat).collect();
                if items.is_empty() { continue; }
                let total: u64 = items.iter().map(|x| x.size).sum();
                let stale: u64 = items.iter().filter(|x| x.suggested(30)).map(|x| x.size).sum();
                println!("{:<20} {:>3} items {:>10} total {:>10} suggested", cat.title(), items.len(), tidy_core::format::bytes(total), tidy_core::format::bytes(stale));
                let mut items = items; items.sort_by(|a, b| b.size.cmp(&a.size));
                for x in items.iter().take(4) {
                    println!("    {:>10}  {}  [{}]{}{}", tidy_core::format::bytes(x.size), x.title, x.detail, if x.keep {" keep"} else {""}, x.blocked.as_deref().map(|b| format!(" BLOCKED: {b}")).unwrap_or_default());
                }
            }
        }
        Some("dupes") => {
            let root = std::path::PathBuf::from(&a[2]);
            let t = tidy_core::scan(&root, Arc::new(tidy_core::Progress::default())).unwrap();
            let home = Some(tidy_core::home_dir());
            let c = tidy_core::dupes::candidates(&t, home.as_deref());
            let n: usize = c.iter().map(|g| g.len()).sum();
            let started = std::time::Instant::now();
            let p = tidy_core::dupes::Progress::default();
            let g = tidy_core::dupes::find(c, home.as_deref(), &p).unwrap();
            let total = p.total.load(std::sync::atomic::Ordering::Relaxed);
            let extra: u64 = g.iter().map(|x| x.extra()).sum();
            println!("{n} candidates, worst case {}, {} groups, {} extra, {} ms", tidy_core::format::bytes(total), g.len(), tidy_core::format::bytes(extra), started.elapsed().as_millis());
            for x in g.iter().take(8) {
                println!("  {:>9} x{}  keep {}", tidy_core::format::bytes(x.size), x.copies.len(), x.copies[0].path.display());
            }
        }
        Some("mcp") => tidy_core::mcp::serve(std::path::Path::new(&a[2])).unwrap(),
        _ => eprintln!("usage: tidy-cli scan <folder> <out.json> | mcp <snapshot.json>"),
    }
}
