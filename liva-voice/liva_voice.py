#!/usr/bin/env python3
"""
LIVA 2.0 - Voice Cloning CLI

Usage:
    python liva_voice.py --url "..." --name "my_voice"
    
    python liva_voice.py --url "..." --name "my_voice" --reference "ref.wav"
"""

import asyncio
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from src.voice_pipeline import VoicePipeline, CloneResult


def print_banner():
    """Print banner"""
    banner = """
    ╔═══════════════════════════════════════════════════════════╗
    ║                                                           ║
    ║   ██╗      ██████╗  ██████╗ ██████╗                     ║
    ║   ██║     ██╔═══██╗██╔════╝ ██╔══██╗                    ║
    ║   ██║     ██║   ██║██║  ███╗██████╔╝                    ║
    ║   ██║     ██║   ██║██║   ██║██╔══██╗                    ║
    ║   ███████╗╚██████╔╝╚██████╔╝██████╔╝                    ║
    ║   ╚══════╝ ╚═════╝  ╚═════╝ ╚═════╝                     ║
    ║                                                           ║
    ║   Version 2.0 - Voice Cloning Pipeline                     ║
    ║                                                           ║
    ╚═══════════════════════════════════════════════════════════╝
    """
    print(banner)


async def main():
    """Main entry point"""
    import argparse
    
    print_banner()
    
    parser = argparse.ArgumentParser(
        description="LIVA 2.0 - Voice Cloning from URL",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Clone from YouTube video
  python liva_voice.py --url "https://youtube.com/watch?v=..." --name "my_voice"
  
  # Clone with reference audio for speaker verification
  python liva_voice.py --url "https://youtube.com/watch?v=..." --name "my_voice" --reference "ref.wav"
  
  # Use custom workspace
  python liva_voice.py --url "..." --name "my_voice" --workspace "./my_workspace"
  
  # Skip speaker verification
  python liva_voice.py --url "..." --name "my_voice" --no-verify
        """
    )
    
    # Required
    parser.add_argument(
        "--url",
        required=True,
        help="Audio URL (YouTube, direct link, etc.)"
    )
    parser.add_argument(
        "--name",
        required=True,
        help="Voice name (used for model filename)"
    )
    
    # Optional
    parser.add_argument(
        "--reference",
        help="Reference audio for speaker verification (5-10s)"
    )
    parser.add_argument(
        "--workspace",
        default="./workspace",
        help="Workspace directory (default: ./workspace)"
    )
    parser.add_argument(
        "--gpt-dir",
        help="GPT-SoVITS directory (optional)"
    )
    parser.add_argument(
        "--no-verify",
        action="store_true",
        help="Skip speaker verification"
    )
    
    # Debug
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug mode"
    )
    
    args = parser.parse_args()
    
    # Validate
    if args.reference and not Path(args.reference).exists():
        print(f"❌ Reference audio not found: {args.reference}")
        sys.exit(1)
    
    # Create workspace
    workspace = Path(args.workspace)
    workspace.mkdir(parents=True, exist_ok=True)
    
    # Create pipeline
    pipeline = VoicePipeline(
        workspace=str(workspace),
        gpt_sovits_dir=args.gpt_dir
    )
    
    print(f"\n📋 Configuration:")
    print(f"   URL: {args.url}")
    print(f"   Voice: {args.name}")
    print(f"   Workspace: {workspace}")
    if args.reference:
        print(f"   Reference: {args.reference}")
    print(f"   Speaker Verify: {not args.no_verify}")
    print()
    
    # Run clone
    try:
        result = await pipeline.clone_voice(
            audio_url=args.url,
            voice_name=args.name,
            reference_audio=args.reference,
            do_speaker_verify=not args.no_verify,
        )
        
        # Print result
        print(f"\n{'='*60}")
        if result.status == "success":
            print(f"✅ CLONE SUCCESS!")
            print(f"")
            print(f"   📦 Model: {result.model_path}")
            print(f"   🎵 Sample: {result.sample_path}")
            print(f"")
            print(f"   📊 Statistics:")
            if result.stats:
                print(f"      Chunks after VAD: {result.stats.get('chunks_after_vad', 'N/A')}")
                print(f"      Chunks after verify: {result.stats.get('chunks_after_verify', 'N/A')}")
                print(f"      Chunks after filter: {result.stats.get('chunks_after_filter', 'N/A')}")
                print(f"      Dataset size: {result.stats.get('dataset_size', 'N/A')}")
            print(f"{'='*60}")
            
            return 0
        else:
            print(f"❌ CLONE FAILED!")
            print(f"")
            print(f"   Error: {result.error}")
            print(f"{'='*60}")
            return 1
            
    except KeyboardInterrupt:
        print(f"\n\n⚠️  Interrupted by user")
        return 130
    except Exception as e:
        print(f"\n\n❌ UNEXPECTED ERROR:")
        print(f"   {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
