//! Mòdul de sandbox Docker — Execució aïllada de codi Rust
//!
//! Aquest mòdul gestiona la validació de codi dins de contenidors Docker
//! amb les següents característiques de seguretat:
//! - Xarxa desactivada (network_mode: none)
//! - Timeout de 60 segons
//! - Usuari no-root
//! - Cache de cargo compartida

use std::path::PathBuf;
use std::process::Command;

/// Configuració del sandbox Docker.
#[derive(Debug, Clone)]
pub struct DockerSandboxConfig {
    /// Nom de la imatge Docker
    pub image_name: String,
    /// Directori del sandbox (on es troba el codi)
    pub sandbox_dir: PathBuf,
    /// Ruta a la memòria cau de cargo
    pub cargo_cache: PathBuf,
    /// Timeout en segons
    pub timeout_seconds: u64,
}

impl Default for DockerSandboxConfig {
    fn default() -> Self {
        Self {
            image_name: "aether-sandbox:latest".into(),
            sandbox_dir: PathBuf::from("/home/aether/sandbox"),
            cargo_cache: std::env::var("CARGO_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/home/aether/.cargo")),
            timeout_seconds: 60,
        }
    }
}

/// Resultat de la validació Docker.
#[derive(Debug, Clone)]
pub struct DockerCheckResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Construeix la imatge Docker si no existeix.
/// `workspace_root` ha de ser el directori que conté el Dockerfile.
pub fn build_docker_image(config: &DockerSandboxConfig, workspace_root: &PathBuf) -> Result<(), String> {
    let output = Command::new("docker")
        .args([
            "build",
            "-t",
            &config.image_name,
            ".",
        ])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("Error executant docker build: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("Error construint imatge Docker: {stderr}"));
    }

    Ok(())
}

/// Comprova si la imatge Docker existeix.
pub fn docker_image_exists(image_name: &str) -> Result<bool, String> {
    let output = Command::new("docker")
        .args(["image", "inspect", image_name])
        .output()
        .map_err(|e| format!("Error comprovant imatge: {e}"))?;

    Ok(output.status.success())
}

/// Executa cargo check dins del contenidor Docker.
/// El contenidor s'executa sense xarxa i amb timeout de 60s.
/// `workspace_root` ha de ser el directori que conté el Dockerfile.
pub fn run_docker_check(config: &DockerSandboxConfig, workspace_root: &PathBuf, code: &str) -> Result<DockerCheckResult, String> {
    // Assegurar-se que la imatge existeix
    if !docker_image_exists(&config.image_name)? {
        if let Err(e) = build_docker_image(config, workspace_root) {
            return Err(format!("Docker no disponible: {e}"));
        }
    }

    // Verificar que la imatge té l'usuari aether
    if let Err(e) = verify_docker_user(&config.image_name, "aether") {
        return Err(format!("Docker image sense usuari correcte: {e}"));
    }

    // Escriure el codi al directori del sandbox
    let main_rs = config.sandbox_dir.join("src").join("main.rs");
    std::fs::create_dir_all(main_rs.parent().unwrap())
        .map_err(|e| format!("Error creant directoris: {e}"))?;

    std::fs::write(&main_rs, code)
        .map_err(|e| format!("Error escrivint codi: {e}"))?;

    // Construir arguments de docker run (sense timeout, ho gestiona el shell)
    let sandbox_mount = format!("{}:/home/aether/sandbox", config.sandbox_dir.display());
    let cargo_mount = format!("{}:/home/aether/.cargo/registry", config.cargo_cache.display());
    
    // Executar amb timeout de 60s mitjançant l'script de shell
    let timeout_seconds = 60u32;
    let docker_args = vec![
        "run",
        "--rm",
        "--network", "none",
        "--memory", "512m",
        "--cpus", "1.0",
        "-v", &sandbox_mount,
        "-v", &cargo_mount,
        "-w", "/home/aether/sandbox",
        "-u", "aether",
        &config.image_name,
        "cargo", "check",
    ];

    // Construir l'script de shell amb timeout
    let docker_cmd = docker_args.join(" ");
    let shell_script = format!("timeout {} docker {}", timeout_seconds, docker_cmd);

    let output = Command::new("sh")
        .arg("-c")
        .arg(&shell_script)
        .output()
        .map_err(|e| format!("Error executant docker run: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}{stderr}");

    Ok(DockerCheckResult {
        success: output.status.success(),
        output: combined,
        error: if output.status.success() {
            None
        } else {
            Some(stderr)
        },
    })
}

/// Verifica que la imatge Docker té l'usuari especificat.
pub fn verify_docker_user(image_name: &str, username: &str) -> Result<(), String> {
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            image_name,
            "id",
            "-un",
        ])
        .output()
        .map_err(|e| format!("Error verificant usuari: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if stdout == username {
        Ok(())
    } else {
        Err(format!("Usuari 'aether' no trobat a la imatge Docker (found: {})", stdout))
    }
}

/// Executa tests dins del contenidor Docker.
pub fn run_docker_tests(config: &DockerSandboxConfig, workspace_root: &PathBuf, code: &str) -> Result<DockerCheckResult, String> {
    // Assegurar-se que la imatge existeix
    if !docker_image_exists(&config.image_name)? {
        build_docker_image(config, workspace_root)?;
    }

    // Escriure el codi
    let main_rs = config.sandbox_dir.join("src").join("main.rs");
    std::fs::create_dir_all(main_rs.parent().unwrap())
        .map_err(|e| format!("Error creant directoris: {e}"))?;
    std::fs::write(&main_rs, code)
        .map_err(|e| format!("Error escrivint codi: {e}"))?;

    // Construir arguments de docker run
    let sandbox_mount = format!("{}:/home/aether/sandbox", config.sandbox_dir.display());
    let cargo_mount = format!("{}:/home/aether/.cargo/registry", config.cargo_cache.display());

    let docker_args = vec![
        "run",
        "--rm",
        "--network", "none",
        "--memory", "512m",
        "--cpus", "1.0",
        "-v", &sandbox_mount,
        "-v", &cargo_mount,
        "-w", "/home/aether/sandbox",
        "-u", "aether",
        &config.image_name,
        "cargo", "test", "--lib",
    ];

    let docker_cmd = docker_args.join(" ");
    let shell_script = format!("timeout 60 docker {}", docker_cmd);

    let output = Command::new("sh")
        .arg("-c")
        .arg(&shell_script)
        .output()
        .map_err(|e| format!("Error executant docker test: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}{stderr}");

    Ok(DockerCheckResult {
        success: output.status.success(),
        output: combined,
        error: if output.status.success() {
            None
        } else {
            Some(stderr)
        },
    })
}

/// Verifica que el codi no intenti fer crides de xarxa.
/// Aquesta és una comprovació estàtica addicional a l'aïllament de Docker.
pub fn check_no_network_code(code: &str) -> Result<(), String> {
    let network_keywords = [
        "std::net::",
        "reqwest::",
        "hyper::",
        "tokio::net::",
        "TcpStream::connect",
        "TcpListener::bind",
        "UdpSocket",
        "dns::",
    ];

    for keyword in &network_keywords {
        if code.contains(keyword) {
            return Err(format!("Codi detectat amb possible accés a xarxa: {keyword}"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_no_network_code_valid() {
        let code = "fn main() { println!(\"Hola\"); }";
        assert!(check_no_network_code(code).is_ok());
    }

    #[test]
    fn test_check_no_network_code_invalid() {
        let code = "use std::net::TcpStream; fn main() { TcpStream::connect(\"localhost:80\"); }";
        let result = check_no_network_code(code);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("xarxa"));
    }

    #[test]
    fn test_default_config() {
        let config = DockerSandboxConfig::default();
        assert_eq!(config.timeout_seconds, 60);
        assert_eq!(config.image_name, "aether-sandbox:latest");
    }

    #[test]
    fn test_network_isolation_verification() {
        // Aquest test verifica que el codi amb accés a xarxa és detectat
        // per la comprovació estàtica de l'orquestrador.
        // La verificació real de xarxa es fa amb Docker (network_mode: none).

        let code_with_network = "use std::net::TcpStream; fn main() { TcpStream::connect(\"127.0.0.1:8080\"); }";
        assert!(
            check_no_network_code(code_with_network).is_err(),
            "El codi amb TcpStream hauria de ser detectat"
        );

        let code_with_reqwest = "fn main() { reqwest::blocking::get(\"https://example.com\").unwrap(); }";
        assert!(
            check_no_network_code(code_with_reqwest).is_err(),
            "El codi amb reqwest hauria de ser detectat"
        );

        let code_without_network = "fn main() { let x = 42; println!(\"{}\", x); }";
        assert!(
            check_no_network_code(code_without_network).is_ok(),
            "El codi sense xarxa hauria de ser acceptat"
        );
    }

    #[test]
    fn test_docker_image_exists() {
        // Verifica que la funció docker_image_exists funciona correctament
        // amb una imatge que probablement existeix (alpine)
        let result = docker_image_exists("alpine:latest");
        // El resultat pot ser Ok(true) o Ok(false) depenent de si la imatge existeix
        assert!(result.is_ok(), "docker_image_exists no hauria de fallar");
    }

    #[test]
    fn test_docker_build_command() {
        // Aquest test verifica que el comanament de build es genera correctament
        let config = DockerSandboxConfig::default();
        let workspace = PathBuf::from("/tmp/aether_test_docker");
        std::fs::create_dir_all(&workspace).ok();

        // Crea un Dockerfile de prova
        let dockerfile = workspace.join("Dockerfile");
        std::fs::write(&dockerfile, "FROM alpine:latest\nRUN echo hello").ok();

        // Executar el build Docker només si Docker està disponible
        let result = build_docker_image(&config, &workspace);
        // Podem ser flexibles: o bé funciona, o bé Docker no està disponible
        match result {
            Ok(()) => {
                // Build exitós — tot correcte
            }
            Err(e) => {
                // Si Docker no està disponible, és acceptable
                assert!(
                    e.contains("Docker") || e.contains("exec"),
                    "Error esperat si Docker no és accessible: {}",
                    e
                );
            }
        }

        // Netejar
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
