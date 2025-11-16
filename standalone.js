import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import os from 'os';
import readline from 'readline';
import fs from 'fs/promises';
import chalk from 'chalk';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout
});

const question = (query) => new Promise((resolve) => rl.question(query, resolve));

const formatNumber = (num) => {
  const numValue = parseFloat(num);
  if (numValue >= 1000000) {
    return (numValue / 1000000).toFixed(1) + 'M';
  } else if (numValue >= 1000) {
    if (numValue % 1000 === 0) {
      return (numValue / 1000).toFixed(0) + 'k';
    } else {
      return numValue.toLocaleString('en-US', { maximumFractionDigits: 0 });
    }
  } else {
    return numValue.toLocaleString('en-US', { maximumFractionDigits: 2 });
  }
};

const saveToFile = async (address, privateKey, totalAttempts, elapsedSeconds, walletsPerSecondFormatted) => {
  const timestamp = new Date().toISOString();
  const entry = `[${timestamp}] Vanity Wallet Found!\n` +
                `Address: ${address}\n` +
                `Private Key: ${privateKey}\n` +
                `Total Attempts: ${totalAttempts.toLocaleString()}\n` +
                `Time Elapsed: ${elapsedSeconds.toFixed(2)}s\n` +
                `Wallets/Second: ${walletsPerSecondFormatted}\n` +
                `---\n\n`;

  try {
    await fs.appendFile('vanity_wallets.txt', entry);
    console.log(chalk.green('Saved to vanity_wallets.txt'));
  } catch (error) {
    console.error(chalk.red('Error saving to file:'), error.message);
  }
};

const handleRareWallet = async (address, privateKey, pattern) => {
  const timestamp = new Date().toISOString();
  const entry = `[${timestamp}] Rare Wallet Found!\n` +
                `Pattern: ${pattern}\n` +
                `Address: ${address}\n` +
                `Private Key: ${privateKey}\n` +
                `---\n\n`;

  try {
    await fs.appendFile('rare_wallets.txt', entry);
    console.log(chalk.yellow('\nRare wallet found!'));
    console.log(chalk.cyan(`   Pattern: ${pattern}`));
    console.log(chalk.cyan(`   Address: ${address}`));
    console.log(chalk.green('   Saved to rare_wallets.txt'));
  } catch (error) {
    console.error(chalk.red('Error saving rare wallet:'), error.message);
  }
};

const startVanitySearch = async (prefix) => {
  const startTime = Date.now();
  const numThreads = os.cpus().length;
  let found = false;

  const binaryPath = process.platform === 'win32' 
    ? join(__dirname, 'vanity_gen', 'target', 'release', 'vanity_gen.exe')
    : join(__dirname, 'vanity_gen', 'target', 'release', 'vanity_gen');

  console.log(chalk.cyan('\nStarting vanity search for prefix:'), chalk.white.bold(`"${prefix}"`));
  console.log(chalk.cyan(`Using ${numThreads} threads\n`));

  const rustProcess = spawn(binaryPath, [], {
    stdio: ['pipe', 'pipe', 'pipe']
  });

  let outputBuffer = '';

  rustProcess.stdout.on('data', async (data) => {
    outputBuffer += data.toString();
    const lines = outputBuffer.split('\n');
    outputBuffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const msg = JSON.parse(line);

        if (msg.type === 'found' && !found) {
          found = true;
          
          const elapsed = Date.now() - startTime;
          const totalAttempts = msg.attempts;
          
          rustProcess.stdin.write('stop\n');
          rustProcess.kill();
          
          const elapsedSeconds = parseFloat((elapsed / 1000).toFixed(2));
          const walletsPerSecondNum = totalAttempts > 0 && elapsedSeconds > 0 
            ? (totalAttempts / elapsedSeconds) 
            : 0;
          const walletsPerSecond = formatNumber(walletsPerSecondNum);
          
          console.log(chalk.green('\nVanity Wallet Found!'));
          console.log(chalk.white(`   Address: ${msg.address}`));
          console.log(chalk.white(`   Private Key: ${msg.private_key}`));
          console.log(chalk.cyan(`   Total Attempts: ${totalAttempts.toLocaleString()}`));
          console.log(chalk.cyan(`   Time Elapsed: ${elapsedSeconds.toFixed(2)}s`));
          console.log(chalk.cyan(`   Wallets/Second: ${walletsPerSecond}`));
          console.log(chalk.cyan(`   Threads Used: ${numThreads}\n`));

          await saveToFile(msg.address, msg.private_key, totalAttempts, elapsedSeconds, walletsPerSecond);

          process.exit(0);
        } else if (msg.type === 'rare') {
          await handleRareWallet(msg.address, msg.private_key, msg.pattern);
        } else if (msg.type === 'progress') {
          const totalAttempts = msg.attempts;
          const elapsed = Date.now() - startTime;
          const elapsedSeconds = elapsed / 1000;
          const walletsPerSecondNum = totalAttempts > 0 && elapsedSeconds > 0 
            ? (totalAttempts / elapsedSeconds) 
            : 0;
          const walletsPerSecond = formatNumber(walletsPerSecondNum);
          
          process.stdout.write(`\r${chalk.yellow('Searching...')} ${chalk.white(`Attempts: ${totalAttempts.toLocaleString()}`)} | ${chalk.white(`Speed: ${walletsPerSecond} wallets/sec`)} | ${chalk.white(`Time: ${elapsedSeconds.toFixed(1)}s`)}`);
        }
      } catch (err) {
        console.error(chalk.red('Error parsing output:'), err.message);
      }
    }
  });

  rustProcess.stderr.on('data', (data) => {
    console.error(chalk.red('Rust process error:'), data.toString());
  });

  rustProcess.on('error', async (error) => {
    console.error(chalk.red('Failed to start Rust process:'), error.message);
    console.error(chalk.yellow('Make sure to run: npm run build:rust'));
    process.exit(1);
  });

  rustProcess.on('close', (code) => {
    if (code !== 0 && code !== null && !found) {
      console.error(chalk.red('Vanity generator process exited unexpectedly.'));
      process.exit(1);
    }
  });

  rustProcess.stdin.write(JSON.stringify({ prefix }) + '\n');
};

const main = async () => {
  console.log(chalk.cyan('=== Solana Vanity Wallet Generator ===\n'));
  
  const prefix = await question(chalk.white('Enter prefix to search for: '));
  
  if (!prefix || prefix.length === 0) {
    console.error(chalk.red('Invalid prefix!'));
    rl.close();
    process.exit(1);
  }

  rl.close();
  await startVanitySearch(prefix);
};

main().catch((error) => {
  console.error(chalk.red('Fatal error:'), error.message);
  process.exit(1);
});
