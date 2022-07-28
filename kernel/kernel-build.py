import argparse
import os
# DEBUG = True
DEBUG = False
# static values
OBJDUMP = "rust-objdump --arch-name=riscv64"
OBJCOPY = "rust-objcopy --binary-architecture=riscv64"
NM = "rust-nm "
READELF = "readelf "
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
    print("The Build Mode Is Change To:"+MODE)

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
    os.system("rm linker.ld.bak")
    os.system("rm dump.txt")
    os.system("rm head.txt")

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
        last_flag_file = last_flag_file.readline()
        if last_flag_file != "":
            last_flag_file = last_flag_file[:-1].split("-")[1]
            p = platforms[last_flag_file]
            p.clean(p)
        os.system("touch "+flag_file)

    os.system("cp platform/"+self.name+"/linker.ld .")

def qemu_build(self):
    print("Build Platform:"+self.name)
    self.pre_build(self)
    if is_debug():
        # return os.system("cargo build --target targets/riscv64.json")==0
        return os.system("cargo build")==0
    else:
        return os.system("cargo build --release")==0

def qemu_after_build(self):
    if not self.build(self):
        return False
    cmd = OBJCOPY+" "+KERNEL_ELF+" --strip-all -O binary " +KERNEL_BIN
    if DEBUG:
        print(cmd)
    os.system(cmd)
    cmd = OBJDUMP+" -S "+KERNEL_ELF+">dump.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    #cmd = OBJDUMP+" -h "+KERNEL_ELF+">head.txt"
    cmd = "objdump -h "+KERNEL_ELF+">head.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    cmd = NM+" "+KERNEL_ELF+">nm.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    cmd = READELF+" --segments "+KERNEL_ELF+">>head.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    return True

def qemu_run(self,debug):
    if not self.after_build(self):
        exit(-1)
    print("build all OK!")
    print("start run...")
    cmd = "qemu-system-riscv64 \
               -machine virt \
              -nographic \
              -bios ../bootloader/rustsbi-qemu.bin \
              -device loader,addr=0x80200000,file="+KERNEL_BIN+" " + \
    "-drive file=fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0"
    if debug:
        cmd += " -S -s"
    os.system(cmd)



# k210 function
def k210_clean(self):
    common_clean(self)

def k210_pre_build(self):
    flag_file = "last-"+self.name
    print(self)
    ret = os.system("cat "+flag_file)
    if ret == 0:
        pass
    else:
        last_flag_file = os.popen("ls last-*")
        last_flag_file = last_flag_file.readline()
        if last_flag_file != "":
            last_flag_file = last_flag_file[:-1].split("-")[1]
            p = platforms[last_flag_file]
            p.clean(p)
        os.system("touch "+flag_file)

    os.system("cp platform/"+self.name+"/linker.ld .")

def k210_build(self):
    print("Build Platform:"+self.name)
    self.pre_build(self)
    if is_debug():
        # return os.system("cargo build --target targets/riscv64.json")==0
        return os.system("cargo build --no-default-features --features \"k210,debug\"")==0
    else:
        return os.system("cargo build --release  --no-default-features --features k210,debug")==0

def k210_after_build(self):
    if not self.build(self):
        return False
    cmd = " cp ../bootloader/rustsbi-k210.bin os.bin"+ \
    "&& "+OBJCOPY+" "+KERNEL_ELF+" --strip-all -O binary " +KERNEL_BIN+ \
    "&& dd if="+KERNEL_BIN+" of=os.bin bs=128k seek=1" \
                          "&& echo 123456 "
    print(cmd)

    if DEBUG:
        print(cmd)
    os.system(cmd)
    cmd = OBJDUMP+" -S "+KERNEL_ELF+">dump.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    #cmd = OBJDUMP+" -h "+KERNEL_ELF+">head.txt"
    cmd = "objdump -h "+KERNEL_ELF+">head.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    cmd = NM+" "+KERNEL_ELF+">nm.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    cmd = READELF+" --segments "+KERNEL_ELF+">>head.txt"
    if DEBUG:
        print(cmd)
    os.system(cmd)

    return True

def k210_run(self,debug):
    if not self.after_build(self):
        exit(-1)
    print("build all OK!")
    print("start run...")
    cmd = "sudo chmod 777 /dev/ttyUSB0"+\
    "&& python3 kflash.py -p /dev/ttyUSB0 -b 1500000 -t os.bin"
    if debug:
        cmd += " -S -s"
    os.system(cmd)




# first of all: check and update environment
env_check()

q = Platform("qemu",pre_build=qemu_pre_build,build=qemu_build,
         after_build=qemu_after_build,clean=qemu_clean,run=qemu_run)
k = Platform("k210",pre_build=k210_pre_build,build=k210_build,
             after_build=k210_after_build,clean=k210_clean,run=k210_run)
parser = argparse.ArgumentParser()
run_build_group = parser.add_mutually_exclusive_group()
parser.add_argument("-p","--platform",help="use --show-platforms option to show support platform list , default "
                                           "platform is qemu",default="qemu")
parser.add_argument("--release",help="build release version",action="store_true")
run_build_group.add_argument("-b","--build",help="build project",action="store_true")
run_build_group.add_argument("-r","--run",help="run project",action="store_true")
run_build_group.add_argument("--graph",help="run project with graph",action="store_true")
run_build_group.add_argument("-c","--clean",help="clean project",action="store_true")
run_build_group.add_argument("-v","--version",help="show project version",action="store_true")
run_build_group.add_argument("-d","--debug",help="run with debug mode",action="store_true")
run_build_group.add_argument("--show-platforms",help="show support platform list",action="store_true")

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
    p.run(p,False)

if args.clean:
    p = platforms[platform]
    p.clean(p)

if args.build:
    p = platforms[platform]
    p.after_build(p)

if args.debug:
    p = platforms[platform]
    p.run(p,True)
