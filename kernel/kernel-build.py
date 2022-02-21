import argparse
import os
DEBUG = True
# static values
OBJDUMP = "rust-objdump --arch-name=riscv64"
OBJCOPY = "rust-objcopy --binary-architecture=riscv64"
TARGET = "riscv64gc-unknown-none-elf"
MODE = "debug" #debug/release
KERNEL_ELF = "target/"+TARGET+"/"+MODE+"/kernel"
KERNEL_BIN = KERNEL_ELF+".bin"

def mode_update():
    global KERNEL_BIN
    global KERNEL_ELF
    global DISASM_TMP
    KERNEL_ELF = "target/"+TARGET+"/"+MODE+"/kernel"
    KERNEL_BIN = KERNEL_ELF+".bin"

def is_debug():
    return MODE=="debug"
def is_relese():
    return MODE=="release"

platforms = {}

def env_check():
    # check target
    r = "rustup target list | grep "+TARGET+"\ \(installed\)"
    r = os.popen(r).read()
    if r == '':
        os.system("rustup target add "+TARGET)

    # check binutils
    if os.system(OBJDUMP+" --version 1>/dev/zero")!=0:
        os.system("rustup component add llvm-tools-preview&&cargo install cargo-binutils")

def default_pre_build():
    pass
def default_build():
    pass
def default_after_build():
    pass
def default_clean():
    pass
def default_run():
    pass
class Platform:
    def __init__(self,name,pre_build =default_pre_build, build =default_build, after_build = default_after_build,clean = default_clean, run = default_run):
        self.name = name
        self.pre_build = pre_build
        self.build = build
        self.after_build = after_build
        self.clean = clean
        self.run = run
        global platforms
        platforms[self.name] = self

# common function
def common_clean(self):
    os.system("cargo clean")
    os.system("rm "+"last-"+self.name)
    os.system("rm linker.ld")
    os.system("rm dump.txt")

# qemu function
def qemu_clean(self):
    common_clean(self)

def qemu_pre_build(self):
    flag_file = "last-"+self.name
    print(self)
    ret = os.system("cat "+flag_file)
    if ret == 0:
        pass
    else:
        last_flag_file = os.popen("ls last-*")
        last_flag_file = last_flag_file.read()
        if last_flag_file != "":
            platforms[last_flag_file].clean()
        os.system("touch "+flag_file)
        os.system("cp platform/"+self.name+"/linker.ld .")

def qemu_build(self):
    print("Build Platform:"+self.name)
    self.pre_build(self)
    if is_debug():
        os.system("cargo build")
    else:
        os.system("cargo build --release")

def qemu_after_build(self):
    self.build(self)
    cmd = OBJCOPY+" "+KERNEL_ELF+" --strip-all -O binary " +KERNEL_BIN
    if DEBUG:
        print(cmd)
    os.system(cmd)
    cmd = OBJDUMP+" -S "+KERNEL_ELF+">dump.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

def qemu_run(self):
    self.after_build(self)
    print("build all OK!")
    print("start run...")
    cmd = "qemu-system-riscv64 \
     -machine virt \
              -nographic \
              -bios ../bootloader/rustsbi-qemu.bin \
              -device loader,file="+KERNEL_BIN+",addr=0x80200000"
    os.system(cmd)

# first of all: check and update environment
env_check()

q = Platform("qemu",pre_build=qemu_pre_build,build=qemu_build,
         after_build=qemu_after_build,clean=qemu_clean,run=qemu_run)

parser = argparse.ArgumentParser()
run_build_group = parser.add_mutually_exclusive_group()
parser.add_argument("-p","--platform",help="use --show-platforms option to show platform list , default "
                                           "platform is qemu",default="qemu")
parser.add_argument("--release",help="build release version",action="store_true")
run_build_group.add_argument("-b","--build",help="build project",action="store_true")
run_build_group.add_argument("-r","--run",help="run project",action="store_true")
run_build_group.add_argument("--graph",help="run project with graph",action="store_true")
run_build_group.add_argument("-c","--clean",help="clean project",action="store_true")
run_build_group.add_argument("-v","--version",help="show project version",action="store_true")
run_build_group.add_argument("--show-platforms",help="show target list",action="store_true")

args = parser.parse_args()
platform = args.platform
if platforms.get(platform) == None:
    print("platform "+platform+" not find,use --show-platforms option to show platform list")
    exit(-1)

if args.release:
    MODE = "release"
    mode_update()

if args.show_platforms:
    for k in platforms:
        print(k)

if args.version:
    os.system("cat version")

if args.run:
    p = platforms[platform]
    p.run(p)

if args.clean:
    p = platforms[platform]
    p.clean(p)

if args.build:
    p = platforms[platform]
    p.build(p)
