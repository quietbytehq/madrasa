// Example parameters for main.bicep. Copy to main.bicepparam and edit; do not commit a file
// that holds a password. The database password is read from the environment here, so this
// file holds no secret:
//
//   export NEXUS_DB_PASSWORD="$(openssl rand -hex 24)"
//   az deployment group create --resource-group madrasa-nexus \
//     --template-file main.bicep --parameters main.example.bicepparam
using './main.bicep'

param name = 'madrasa-nexus'
param image = 'ghcr.io/quietbytehq/madrasa-nexus:0.4.3-alpha'
param dbAdminPassword = readEnvironmentVariable('NEXUS_DB_PASSWORD')
// Optional. Empty: the hub prints a generated setup token once in its log.
param setupToken = readEnvironmentVariable('NEXUS_SETUP_TOKEN', '')
// Optional. Your own domain, once it points at the Container App.
param publicUrl = ''
