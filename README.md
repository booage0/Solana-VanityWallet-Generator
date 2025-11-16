# Solana Vanity Wallet Generator

A command-line tool that generates Solana wallet addresses with custom prefixes. You can pick any pattern you want, and the tool will search through millions of generated wallets until it finds one that matches your desired prefix.

For example, instead of a random address like `7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU`, you can have one that starts with your chosen text like `booAgeFR7dKivYZb9w3kR911Up5caXwbXQ9msCYtu6`.

## Performance

The generator can test approximately **250,000 to 500,000+ wallet addresses per second**, depending on your hardware specifications (CPU model, number of cores, clock speed, and SIMD support). Faster hardware with more CPU cores and AVX2/AVX512 support will generate addresses more quickly.

**Important:** The generator uses all available CPU cores at maximum capacity, which consumes significant system resources. It is **not recommended** to use your computer for other tasks while the generator is running, as this may slow down the generation process.

### Performance Optimizations

The generator includes several optimizations:
- **Fast Base58 encoding** using Jump Crypto's Firedancer-optimized `fd_bs58` library (9-13x faster than standard Base58)
- **SIMD acceleration** for cryptographic operations on x86_64 CPUs with AVX2/AVX512 support
- **Case-sensitive matching** for precise vanity pattern control
- **Optimized progress reporting** to minimize overhead

The build script automatically enables CPU-specific optimizations (`target-cpu=native`) when building in release mode. For maximum performance on x86_64 systems, ensure your CPU supports AVX2 or AVX512 instructions.

## Security

# 🔒 ALL PROCESSING HAPPENS LOCALLY ON YOUR MACHINE

**Your private keys never leave your computer.** No data is sent over the internet, and no external servers are involved. Everything runs entirely on your local machine, ensuring complete privacy and security.

## Requirements

- Node.js (version 20 or higher)
- Rust (install from [rustup.rs](https://rustup.rs/))

**Installing Rust:**
- **Windows**: Download and run the installer from [rustup.rs](https://rustup.rs/)
- **macOS/Linux**: Run `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` in your terminal

## Installation

1. Download or clone this repository to your computer

2. Install Node.js dependencies:
```bash
npm install
```

3. Build the Rust generator:
```bash
npm run build:rust
```

This step may take a few minutes the first time.

## Usage

Run the generator:
```bash
npm start
```

When prompted, enter the prefix you want to search for. For example, type `boo` if you want an address starting with "boo" and press Enter.

**Note:** Prefix matching is case-sensitive. If you enter `nub` (lowercase), the generator will only match addresses starting with lowercase "nub". Enter `Nub` to match addresses starting with "Nub" (capital N).

The tool will display real-time progress showing:
- Number of addresses tested
- Generation speed (wallets per second)
- Search duration

When a match is found, the address and private key will be displayed and saved to `vanity_wallets.txt`.

## Configuration

You can configure the tool to detect and save rare patterns by editing `config.json`. Simply list patterns and their minimum lengths:

```json
{
  "patterns": [
    {
      "pattern": "A",
      "minLength": 8
    },
    {
      "pattern": "69",
      "minLength": 5
    }
  ]
}
```

- Single character patterns (like `"A"`) find consecutive repeated characters
- Multi-character patterns (like `"69"`) find repeating sequences

Rare pattern matches are saved to `rare_wallets.txt`.

## Output Files

- **`vanity_wallets.txt`** - Contains found vanity addresses with private keys, timestamps, and statistics
- **`rare_wallets.txt`** - Contains addresses matching your configured rare patterns

## Security Notes

**Keep your private keys safe!** Anyone with access to your private key has full control over your wallet and funds. Never share your private keys or commit them to version control. Treat the output files like passwords.

## Troubleshooting

**"Failed to start Rust process"** - Run `npm run build:rust` to compile the Rust generator first.

**"Rust not found"** - Install Rust from [rustup.rs](https://rustup.rs/) and restart your terminal.

**Slow performance** - Check your CPU usage - it should be at 100% on all cores. Performance varies based on hardware. Close other applications for best results. Ensure you've built with `npm run build:rust` to enable release optimizations.

**Performance tuning** - For profiling and optimization, you can use tools like `cargo flamegraph` to identify bottlenecks. The generator is optimized for x86_64 CPUs with SIMD support. On non-x86_64 architectures, performance will be lower but still functional.

## License

MIT
