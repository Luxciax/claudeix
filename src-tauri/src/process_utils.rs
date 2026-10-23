use std::process::Command;

/// Prevent background console processes from flashing a Windows command window.
#[cfg(windows)]
pub fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console(_command: &mut Command) {}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn background_powershell_has_no_console_and_preserves_output() {
        let mut command = std::process::Command::new("powershell.exe");
        super::hide_console(&mut command);
        command.args(["-NoProfile", "-NonInteractive", "-Command", r#"Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class ConsoleProbe { [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow(); }'; if ([ConsoleProbe]::GetConsoleWindow() -ne [IntPtr]::Zero) { exit 9 }; Write-Output 'no-console'"#]);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "no-console");
    }
}
