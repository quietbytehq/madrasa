// Madrasa Nexus hub on Azure: one Container App and an Azure Database for PostgreSQL
// Flexible Server, with logs in Log Analytics.
//
//   az group create --name madrasa-nexus --location <region>
//   az deployment group create --resource-group madrasa-nexus \
//     --template-file main.bicep --parameters main.example.bicepparam
//
// See docs/nexus/deploy.md. This template is checked by CI (`az bicep build`) but has not
// been deployed by the project: treat the first deployment as a test.

targetScope = 'resourceGroup'

@description('Azure region. Defaults to the resource group\'s region.')
param location string = resourceGroup().location

@description('Short name used for every resource: lower-case letters, digits and hyphens.')
@minLength(3)
@maxLength(20)
param name string = 'madrasa-nexus'

@description('The hub image with a release tag, for example ghcr.io/quietbytehq/madrasa-nexus:0.5.0-alpha (release tags end in -alpha). Avoid "latest": an upgrade should be a decision.')
param image string

@description('PostgreSQL administrator login.')
param dbAdminLogin string = 'nexus'

@description('PostgreSQL administrator password. Letters and digits only: it becomes part of a connection URL.')
@secure()
@minLength(16)
param dbAdminPassword string

@description('One-time token for first-run setup. Leave empty and the hub generates one and prints it once in its log.')
@secure()
param setupToken string = ''

@description('The address schools type, when you put your own domain in front (https://nexus.example.org). Empty: the Container App\'s own address.')
param publicUrl string = ''

@description('PostgreSQL compute size. Standard_B1ms is enough for a first network of a few schools.')
param dbSkuName string = 'Standard_B1ms'

@description('PostgreSQL compute tier.')
@allowed([
  'Burstable'
  'GeneralPurpose'
  'MemoryOptimized'
])
param dbSkuTier string = 'Burstable'

@description('PostgreSQL storage in GB.')
@minValue(32)
param dbStorageGb int = 32

@description('Days of automatic database backups kept by Azure.')
@minValue(7)
@maxValue(35)
param dbBackupRetentionDays int = 14

@description('CPU cores for the hub container.')
param cpu string = '0.5'

@description('Memory for the hub container.')
param memory string = '1Gi'

var hubPort = 4780
var databaseName = 'nexus'
// Server names are global in Azure; the suffix keeps this one unique to the resource group.
var dbServerName = '${name}-db-${uniqueString(resourceGroup().id)}'
var databaseUrl = 'postgres://${dbAdminLogin}:${dbAdminPassword}@${db.properties.fullyQualifiedDomainName}:5432/${databaseName}?sslmode=require'
var publicAddress = empty(publicUrl) ? 'https://${name}.${containerEnv.properties.defaultDomain}' : publicUrl
var baseSecrets = [
  {
    name: 'database-url'
    value: databaseUrl
  }
]
var setupSecrets = empty(setupToken) ? [] : [
  {
    name: 'setup-token'
    value: setupToken
  }
]
var baseEnvironment = [
  {
    name: 'NEXUS_DATABASE_URL'
    secretRef: 'database-url'
  }
  {
    name: 'NEXUS_BIND'
    value: '0.0.0.0:${hubPort}'
  }
  {
    name: 'NEXUS_PUBLIC_URL'
    value: publicAddress
  }
]
var setupEnvironment = empty(setupToken) ? [] : [
  {
    name: 'NEXUS_SETUP_TOKEN'
    secretRef: 'setup-token'
  }
]

resource logs 'Microsoft.OperationalInsights/workspaces@2023-09-01' = {
  name: '${name}-logs'
  location: location
  properties: {
    sku: {
      name: 'PerGB2018'
    }
    retentionInDays: 30
  }
}

resource containerEnv 'Microsoft.App/managedEnvironments@2024-03-01' = {
  name: '${name}-env'
  location: location
  properties: {
    appLogsConfiguration: {
      destination: 'log-analytics'
      logAnalyticsConfiguration: {
        customerId: logs.properties.customerId
        sharedKey: logs.listKeys().primarySharedKey
      }
    }
  }
}

resource db 'Microsoft.DBforPostgreSQL/flexibleServers@2024-08-01' = {
  name: dbServerName
  location: location
  sku: {
    name: dbSkuName
    tier: dbSkuTier
  }
  properties: {
    version: '16'
    administratorLogin: dbAdminLogin
    administratorLoginPassword: dbAdminPassword
    storage: {
      storageSizeGB: dbStorageGb
    }
    backup: {
      backupRetentionDays: dbBackupRetentionDays
      geoRedundantBackup: 'Disabled'
    }
    highAvailability: {
      mode: 'Disabled'
    }
    network: {
      publicNetworkAccess: 'Enabled'
    }
  }
}

// The server has a public endpoint that accepts connections from Azure services only (the
// special 0.0.0.0 rule) and only over TLS with the password. For a private endpoint
// instead, put the server and the Container Apps environment in a virtual network.
resource dbAllowAzure 'Microsoft.DBforPostgreSQL/flexibleServers/firewallRules@2024-08-01' = {
  parent: db
  name: 'AllowAzureServices'
  properties: {
    startIpAddress: '0.0.0.0'
    endIpAddress: '0.0.0.0'
  }
}

resource database 'Microsoft.DBforPostgreSQL/flexibleServers/databases@2024-08-01' = {
  parent: db
  name: databaseName
  properties: {
    charset: 'UTF8'
    collation: 'en_US.utf8'
  }
}

resource hub 'Microsoft.App/containerApps@2024-03-01' = {
  name: name
  location: location
  dependsOn: [
    database
    dbAllowAzure
  ]
  properties: {
    managedEnvironmentId: containerEnv.id
    configuration: {
      activeRevisionsMode: 'Single'
      ingress: {
        external: true
        targetPort: hubPort
        transport: 'auto'
        allowInsecure: false
      }
      secrets: concat(baseSecrets, setupSecrets)
    }
    template: {
      containers: [
        {
          name: 'nexus'
          image: image
          resources: {
            cpu: json(cpu)
            memory: memory
          }
          env: concat(baseEnvironment, setupEnvironment)
          probes: [
            {
              type: 'Startup'
              httpGet: {
                path: '/api/nexus/v1/health'
                port: hubPort
              }
              periodSeconds: 5
              failureThreshold: 30
            }
            {
              type: 'Liveness'
              httpGet: {
                path: '/api/nexus/v1/health'
                port: hubPort
              }
              periodSeconds: 30
            }
          ]
        }
      ]
      // The alpha hub runs as exactly one instance: it keeps a clock of its own, and two
      // instances would share it.
      scale: {
        minReplicas: 1
        maxReplicas: 1
      }
    }
  }
}

@description('The hub\'s address: open it in a browser for first-run setup, and give it to schools.')
output hubUrl string = 'https://${hub.properties.configuration.ingress.fqdn}'

@description('Host name of the PostgreSQL server.')
output databaseHost string = db.properties.fullyQualifiedDomainName
