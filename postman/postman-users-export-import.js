#!/usr/bin/env node

/**
 * Postman Users Export/Import Utility
 * 
 * This script helps export and import saved users from Postman environment files
 * for the centralized-exchange automated testing.
 * 
 * Usage:
 *   node postman-users-export-import.js export <environment-file> <output-file>
 *   node postman-users-export-import.js import <environment-file> <users-file>
 */

const fs = require('fs');
const path = require('path');

function printUsage() {
    console.log(`
Postman Users Export/Import Utility

Usage:
  Export users: node ${path.basename(process.argv[1])} export <environment-file> <output-file>
  Import users: node ${path.basename(process.argv[1])} import <environment-file> <users-file>

Examples:
  node ${path.basename(process.argv[1])} export MyEnvironment.postman_environment.json users.json
  node ${path.basename(process.argv[1])} import MyEnvironment.postman_environment.json users.json
`);
}

function exportUsers(envFile, outputFile) {
    try {
        // Read environment file
        const envData = JSON.parse(fs.readFileSync(envFile, 'utf8'));
        
        // Find savedUsers variable
        const savedUsersVar = envData.values.find(v => v.key === 'savedUsers');
        
        if (!savedUsersVar || !savedUsersVar.value) {
            console.error('Error: No savedUsers found in the environment file');
            process.exit(1);
        }
        
        // Parse users data
        const users = JSON.parse(savedUsersVar.value);
        
        // Create export object with metadata
        const exportData = {
            exportDate: new Date().toISOString(),
            userCount: users.length,
            environmentName: envData.name,
            users: users
        };
        
        // Write to output file
        fs.writeFileSync(outputFile, JSON.stringify(exportData, null, 2));
        
        console.log(`✓ Successfully exported ${users.length} users to ${outputFile}`);
        console.log(`  Export date: ${exportData.exportDate}`);
        console.log(`  Environment: ${exportData.environmentName}`);
        
    } catch (error) {
        console.error('Error exporting users:', error.message);
        process.exit(1);
    }
}

function importUsers(envFile, usersFile) {
    try {
        // Read environment file
        const envData = JSON.parse(fs.readFileSync(envFile, 'utf8'));
        
        // Read users file
        const importData = JSON.parse(fs.readFileSync(usersFile, 'utf8'));
        
        // Extract users array (support both direct array and export object)
        const users = Array.isArray(importData) ? importData : importData.users;
        
        if (!Array.isArray(users) || users.length === 0) {
            console.error('Error: Invalid users data in import file');
            process.exit(1);
        }
        
        // Find or create savedUsers variable
        let savedUsersVar = envData.values.find(v => v.key === 'savedUsers');
        
        if (!savedUsersVar) {
            savedUsersVar = {
                key: 'savedUsers',
                value: '',
                type: 'default',
                enabled: true
            };
            envData.values.push(savedUsersVar);
        }
        
        // Update the value
        savedUsersVar.value = JSON.stringify(users);
        
        // Update related variables
        updateOrAddVariable(envData, 'savedUsersCount', users.length.toString());
        updateOrAddVariable(envData, 'savedUsersDate', new Date().toISOString());
        updateOrAddVariable(envData, 'skipRegistration', 'true');
        
        // Create backup of original file
        const backupFile = envFile.replace('.json', `.backup-${Date.now()}.json`);
        fs.writeFileSync(backupFile, fs.readFileSync(envFile));
        
        // Write updated environment file
        fs.writeFileSync(envFile, JSON.stringify(envData, null, 2));
        
        console.log(`✓ Successfully imported ${users.length} users to ${envFile}`);
        console.log(`  Backup created: ${backupFile}`);
        console.log(`  skipRegistration set to: true`);
        
        if (importData.exportDate) {
            console.log(`  Original export date: ${importData.exportDate}`);
        }
        
    } catch (error) {
        console.error('Error importing users:', error.message);
        process.exit(1);
    }
}

function updateOrAddVariable(envData, key, value) {
    let variable = envData.values.find(v => v.key === key);
    
    if (!variable) {
        variable = {
            key: key,
            value: value,
            type: 'default',
            enabled: true
        };
        envData.values.push(variable);
    } else {
        variable.value = value;
    }
}

// Main execution
const args = process.argv.slice(2);

if (args.length < 3) {
    printUsage();
    process.exit(1);
}

const command = args[0];
const envFile = args[1];
const dataFile = args[2];

// Check if environment file exists
if (!fs.existsSync(envFile)) {
    console.error(`Error: Environment file '${envFile}' not found`);
    process.exit(1);
}

switch (command) {
    case 'export':
        exportUsers(envFile, dataFile);
        break;
    case 'import':
        if (!fs.existsSync(dataFile)) {
            console.error(`Error: Users file '${dataFile}' not found`);
            process.exit(1);
        }
        importUsers(envFile, dataFile);
        break;
    default:
        console.error(`Error: Unknown command '${command}'`);
        printUsage();
        process.exit(1);
} 