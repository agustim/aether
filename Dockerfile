# Fem servir la versió slim de Rust (molt més petita que la completa però amb tot el necessari)
FROM rust:1.75-slim

# Instal·lem eines bàsiques si les necessites (opcional)
RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*

# Creem l'usuari 'aether' de forma estàndard Linux
RUN useradd -m -s /bin/bash aether

# Configurem el directori de treball
WORKDIR /home/aether/app

# Donem permisos totals a l'usuari aether sobre la seva carpeta
RUN chown -R aether:aether /home/aether/app

# Canviem a l'usuari no-privilegiat
USER aether

# Preparem un projecte buit perquè les compilacions posteriors siguin ràpides
RUN cargo new --bin sandbox
WORKDIR /home/aether/app/sandbox

# Per defecte, si no li diem res, que no faci res pesat
CMD ["sleep", "infinity"]