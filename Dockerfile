# Aether Code Docker Sandbox
# Imatge lleugera per a validació de codi Rust aïllada

FROM rust:1.75-slim

# Desactivar xarxa per defecte
ENV NETWORK_DISABLED=1

# Crear usuari no-root per seguretat
RUN useradd -m -u 1000 aether

# Canviar a usuari no-root
USER aether

# Directori de treball
WORKDIR /home/aether/sandbox

# Per defecte, executar cargo check
CMD ["cargo", "check"]
