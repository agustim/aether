# Aether Code Docker Sandbox — Imatge mínima per a validació aïllada
FROM rust:1.75-slim

# Instal·lar dependències necessàries
RUN apt-get update && apt-get install -y --no-install-recommends gcc && rm -rf /var/lib/apt/lists/*

# Crear usuari no-root per seguretat
RUN useradd -m -u 1000 aether

# Canviar a usuari no-root
USER aether

# Directori de treball
WORKDIR /home/aether/sandbox

# Per defecte, executar cargo check
CMD ["cargo", "check"]
