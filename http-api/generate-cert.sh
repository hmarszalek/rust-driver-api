#!/bin/bash

# Exit on any error
set -e

echo "=== Generating TLS Certificates for ScyllaDB Cluster ==="
echo ""

# Create certs directory
mkdir -p certs
cd certs

echo "Step 1: Generating CA (Certificate Authority) key and certificate..."
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days 3650 -key ca.key -out ca.crt \
  -subj "/C=US/ST=State/L=City/O=Organization/CN=ScyllaDB-CA"
echo "✓ CA certificate created"
echo ""

# Function to generate node certificates
generate_node_cert() {
  local node_name=$1
  local internal_ip=$2
  local external_ip=$3

  echo "Step: Generating certificate for $node_name (internal: $internal_ip, external: $external_ip)..."

  # Generate node key
  openssl genrsa -out ${node_name}.key 4096

  # Generate CSR (Certificate Signing Request)
  openssl req -new -key ${node_name}.key -out ${node_name}.csr \
    -subj "/C=US/ST=State/L=City/O=Organization/CN=${node_name}"

  # Create extension file for SAN (Subject Alternative Names)
  # Include BOTH the internal Docker IP AND the external localhost IP
  cat > ${node_name}.ext << EOF
subjectAltName = IP:${internal_ip},IP:${external_ip},IP:127.0.0.1,DNS:${node_name},DNS:localhost
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
EOF

  # Sign the certificate with CA
  openssl x509 -req -in ${node_name}.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out ${node_name}.crt -days 3650 \
    -extfile ${node_name}.ext

  # Clean up temporary files
  rm ${node_name}.csr ${node_name}.ext

  echo "✓ Certificate created for $node_name"
  echo "  - Internal IP: $internal_ip"
  echo "  - External IP: $external_ip"
  echo ""
}

# Generate certificates for each ScyllaDB node
# Format: generate_node_cert "name" "docker_network_ip" "localhost_mapped_ip"
generate_node_cert "scylla1" "172.42.0.2" "127.0.0.2"
generate_node_cert "scylla2" "172.42.0.3" "127.0.0.3"
generate_node_cert "scylla3" "172.42.0.4" "127.0.0.4"

# Fix permissions - make files readable by ScyllaDB container
echo "Step: Setting proper file permissions..."
chmod 644 *.crt *.key

echo "✓ Permissions set"
echo ""
echo "=== Certificate generation complete! ==="
echo ""
echo "Generated files in ./certs/:"
ls -lh
echo ""
echo "Verifying certificate SANs:"
echo ""
for node in scylla1 scylla2 scylla3; do
  echo "=== $node certificate includes: ==="
  openssl x509 -in ${node}.crt -text -noout | grep -A1 "Subject Alternative Name"
  echo ""
done
echo ""
echo "Next steps:"
echo "1. Run: docker-compose down"
echo "2. Run: docker-compose up -d"
echo "3. Wait 120 seconds for cluster to start"
echo "4. Run: cargo run"
