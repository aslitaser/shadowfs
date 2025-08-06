use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};
use std::io::{self, Write};

use super::Detector;

pub fn print_colored(text: &str, color: Color) {
    let mut stdout = io::stdout();
    stdout.execute(SetForegroundColor(color)).unwrap();
    print!("{}", text);
    stdout.execute(ResetColor).unwrap();
    stdout.flush().unwrap();
}

pub fn print_box(title: &str, content: Vec<(&str, String, bool)>, width: usize) {
    let mut stdout = io::stdout();
    
    stdout.execute(SetForegroundColor(Color::Cyan)).unwrap();
    println!("┌─ {} {}", title, "─".repeat(width.saturating_sub(title.len() + 4)));
    
    for (label, value, success) in content {
        stdout.execute(SetForegroundColor(Color::White)).unwrap();
        print!("│ ");
        
        if success {
            stdout.execute(SetForegroundColor(Color::Green)).unwrap();
            print!("✅ ");
        } else {
            stdout.execute(SetForegroundColor(Color::Yellow)).unwrap();
            print!("⚠️  ");
        }
        
        stdout.execute(SetForegroundColor(Color::White)).unwrap();
        print!("{}: ", label);
        stdout.execute(SetForegroundColor(Color::Cyan)).unwrap();
        println!("{}", value);
    }
    
    stdout.execute(SetForegroundColor(Color::Cyan)).unwrap();
    println!("└{}", "─".repeat(width));
    stdout.execute(ResetColor).unwrap();
}

pub async fn run_cli() {
    print_colored("🔍 ShadowFS Platform Detection\n", Color::Magenta);
    println!("─────────────────────────────────────");
    
    print_colored("Detecting platform capabilities...\n\n", Color::Cyan);
    
    let detector = Detector::new();
    let info = match detector.detect_all() {
        Ok(report) => report.system_info,
        Err(_) => {
            println!("Error: Failed to detect platform information");
            return;
        }
    };
    
    let mut content = vec![
        ("Platform", format!("{:?}", info.platform), true),
        ("Architecture", format!("{:?}", info.architecture), true),
        ("OS Version", info.version.to_string(), true),
    ];
    
    // Add kernel version if available
    if let Some(kernel) = &info.kernel_version {
        content.push(("Kernel", kernel.to_string(), true));
    }
    
    // Add filesystem backend status
    #[cfg(target_os = "macos")]
    {
        content.push(("", String::new(), false));
        
        // Check for FSKit
        use super::macos_detector::MacOSDetector;
        let macos_detector = MacOSDetector::new();
        if let Ok(fskit) = macos_detector.detect_fskit() {
            if fskit.available {
                let version = fskit.version.map(|v| v.to_string()).unwrap_or_else(|| "Unknown".to_string());
                content.push(("FSKit", format!("Available ({})", version), true));
            } else {
                content.push(("FSKit", "Not Available".to_string(), false));
            }
        }
        
        // Check for macFUSE
        if let Ok(macfuse) = macos_detector.detect_macfuse() {
            if macfuse.installed {
                let version = macfuse.version.map(|v| v.to_string()).unwrap_or_else(|| "Unknown".to_string());
                content.push(("macFUSE", format!("Installed ({})", version), true));
            } else {
                content.push(("macFUSE", "Not Installed".to_string(), false));
            }
        }
        
        content.push(("", String::new(), false));
        content.push(("Recommended", "FSKit (best performance)".to_string(), true));
    }
    
    #[cfg(target_os = "linux")]
    {
        content.push(("", String::new(), false));
        
        // Check for FUSE
        use super::linux_detector::LinuxDetector;
        let linux_detector = LinuxDetector::new();
        if let Ok(fuse) = linux_detector.detect_fuse() {
            if fuse.installed {
                let version = fuse.version.map(|v| v.to_string()).unwrap_or_else(|| "Unknown".to_string());
                content.push(("FUSE", format!("Available (v{})", version), true));
            } else {
                content.push(("FUSE", "Not Available".to_string(), false));
            }
        }
        
        content.push(("", String::new(), false));
        content.push(("Recommended", "FUSE3 (stable)".to_string(), true));
    }
    
    #[cfg(target_os = "windows")]
    {
        content.push(("", String::new(), false));
        
        // Check for ProjFS
        use super::windows_detector::WindowsDetector;
        let windows_detector = WindowsDetector::new();
        if let Ok(projfs) = windows_detector.detect_projfs() {
            if projfs.available {
                let version = projfs.version.to_string();
                content.push(("ProjFS", format!("Available ({})", version), true));
            } else {
                content.push(("ProjFS", "Not Available".to_string(), false));
            }
        }
        
        content.push(("", String::new(), false));
        content.push(("Recommended", "ProjFS (native)".to_string(), true));
    }
    
    print_box("ShadowFS Platform Detection", content, 50);
    
    println!("\n");
    print_colored("Next Steps:\n", Color::Green);
    println!("1. Run capability tests: shadowfs-detect test");
    println!("2. Install recommended backend: shadowfs-detect install");
    println!("3. Diagnose issues: shadowfs-detect doctor");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cli_runs() {
        // Just ensure it doesn't panic
        run_cli().await;
    }
}