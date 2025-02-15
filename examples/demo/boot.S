# boot.S
# bootloader for SoS
# Stephen Marz
# 8 February 2019

# Disable generation of compressed instructions.
.option norvc

# Define a .text.init section. The .text.init is put at the
# starting address so that the entry _start is put at the RISC-V
# address 0x8000_0000.
.section .text.init

# Execution starts here.
.global _start
_start:

	# Disable linker instruction relaxation for the `la` instruction below.
	# This disallows the assembler from assuming that `gp` is already initialized.
	# This causes the value stored in `gp` to be calculated from `pc`.
	# The job of the global pointer is to give the linker the ability to address
	# memory relative to GP instead of as an absolute address.
.option push
.option norelax
	la		gp, _global_pointer
.option pop
	# The stack grows from bottom to top, so we put the stack pointer
	# to the very end of the stack range.
	la		sp, _stack_end
    # Trap vector base address
    la      t0, handler
    csrw    mtvec, t0
	# Machine's exception program counter (MEPC) is set to `main`.
	la		t1, main
	csrw	mepc, t1
    mret


	.align 2
handler:
0:  j   0b
