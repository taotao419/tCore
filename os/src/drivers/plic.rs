#[allow(clippy::upper_case_acronyms)]
pub struct PLIC {
    base_addr: usize,
}

#[derive(Copy, Clone)]
pub enum IntrTargetPriority {
    Machine = 0,
    Supervisor = 1,
}

impl IntrTargetPriority {
    pub fn supported_number() -> usize {
        return 2;
    }
}

impl PLIC {
    pub unsafe fn new(base_addr: usize) -> Self {
        Self { base_addr }
    }

    // 根据中断源id 获取中断优先级寄存器的地址(指针)
    fn priority_ptr(&self, intr_source_id: usize) -> *mut u32 {
        assert!(intr_source_id > 0 && intr_source_id <= 132);
        // 根据 PLIC Specification
        // 0x000000: Reserved (interrupt source 0 does not exist)
        // 0x000004: Interrupt source 1 priority
        // 0x000008: Interrupt source 2 priority
        (self.base_addr + intr_source_id * 4) as *mut u32
    }

    fn hart_id_with_priority(hart_id: usize, target_priority: IntrTargetPriority) -> usize {
        let priority_num = IntrTargetPriority::supported_number();
        return hart_id * priority_num + target_priority as usize;
    }

    // 根据 hart_id + target_priority aka(context_id) 与中断源id 获取使能寄存器的地址(指针)
    fn enable_ptr(
        &self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        intr_source_id: usize,
    ) -> (*mut u32, usize) {
        let id = Self::hart_id_with_priority(hart_id, target_priority);
        // 根据 PLIC Specification
        // 0x002000: Interrupt Source #0 to #31 Enable Bits on context 0
        // ...
        // 0x00207C: Interrupt Source #992 to #1023 Enable Bits on context 0
        // 0x002080: Interrupt Source #0 to #31 Enable Bits on context 1
        // ...
        // 0x0020FC: Interrupt Source #992 to #1023 Enable Bits on context 1
        // 0x002100: Interrupt Source #0 to #31 Enable Bits on context 2
        // ...
        // 0x00217C: Interrupt Source #992 to #1023 Enable Bits on context 2
        // 这里肯定中断源总数不超过32个, 就是为了算出 #n Enable Bits on context X
        let (reg_id, reg_shift) = (intr_source_id / 32, intr_source_id % 32);
        (
            //PLIC Specification: The base address of Interrupt Enable Bits block within PLIC Memory Map region is fixed at 0x002000.
            //这里0x80 也是根据上面Specification中每个context X的间隔获知的.
            //这里0x4  #0~#31 即 4*1byte == 4*8bit
            //reg_shift 表示第几个中断源
            (self.base_addr + 0x2000 + 0x80 * id + 0x4 * reg_id) as *mut u32,
            reg_shift,
        )
    }

    // 根据 hart_id + target_priority 即(context_id) , 获取中断目标阈值寄存器的地址(指针)
    fn threshold_ptr_of_hart_with_priority(
        &self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
    ) -> *mut u32 {
        let id = Self::hart_id_with_priority(hart_id, target_priority);
        // 根据 PLIC Specification
        // The base address of Priority Thresholds register block is located at 4K alignment starts from offset 0x200000.
        // 0x200000: Priority threshold for context 0
        // 0x201000: Priority threshold for context 1
        // 0x202000: Priority threshold for context 2
        // 0x203000: Priority threshold for context 3
        return (self.base_addr + 0x20_0000 + 0x1000 * id) as *mut u32;
    }

    // 根据 hart_id + target_priority 即(context_id) , 获取 claim/ complete 寄存器地址 (指针)
    fn claim_comp_ptr_of_hart_with_priority(
        &self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
    ) -> *mut u32 {
        let id = Self::hart_id_with_priority(hart_id, target_priority);
        // 根据 PLIC Specification
        // The Interrupt Claim Process register is context based and is located at (4K alignment + 4) starts from offset 0x200000.
        // 0x200004: Interrupt Claim Process for context 0
        // 0x201004: Interrupt Claim Process for context 1
        // 0x202004: Interrupt Claim Process for context 2
        // 0x203004: Interrupt Claim Process for context 3
        return (self.base_addr + 0x20_0004 + 0x1000 * id) as *mut u32;
    }

    // 中断源设置优先级
    pub fn set_priority(&mut self, intr_source_id: usize, priority: u32) {
        assert!(priority < 8);
        unsafe {
            self.priority_ptr(intr_source_id).write_volatile(priority);
        }
    }

    // 读取中断源优先级
    #[allow(unused)]
    pub fn get_priority(&mut self, intr_source_id: usize) -> u32 {
        // 由于优先级只可以为 0~7, 所以从寄存器读出的32值, . 用 & 0b0111 (7) 运算是取交集. 意思就是寄存器的值只看最后3位
        unsafe { self.priority_ptr(intr_source_id).read_volatile() & 7 }
    }

    // 使能中断源
    pub fn enable(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        intr_source_id: usize,
    ) {
        let (reg_ptr, shift) = self.enable_ptr(hart_id, target_priority, intr_source_id);
        // 1 << shift 表示 第几个bit为1, 其他为0
        // Ex : 如下例子表示第3个与第30个 中断源使能  & 第992个 中断源使能
        //0x002000:  00000100
        //0x002001:  00000000
        //0x002002:  00000000
        //0x002003:  01000000
        // ....
        //0x00207C:  00000001
        unsafe {
            reg_ptr.write_volatile(reg_ptr.read_volatile() | 1 << shift);
        }
    }

    // 禁用中断源
    #[allow(unused)]
    pub fn disable(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        intr_source_id: usize,
    ) {
        let (reg_ptr, shift) = self.enable_ptr(hart_id, target_priority, intr_source_id);
        unsafe {
            // !(1u32 << shift) 把第N位置的 1==>0
            reg_ptr.write_volatile(reg_ptr.read_volatile() & (!(1u32 << shift)));
        }
    }

    // 设置阈值
    pub fn set_threshold(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        threshold: u32,
    ) {
        assert!(threshold < 8);
        let threshold_ptr = self.threshold_ptr_of_hart_with_priority(hart_id, target_priority);
        unsafe {
            threshold_ptr.write_volatile(threshold);
        }
    }

    //读取阈值
    #[allow(unused)]
    pub fn get_threshold(&mut self, hart_id: usize, target_priority: IntrTargetPriority) -> u32 {
        let threshold_ptr = self.threshold_ptr_of_hart_with_priority(hart_id, target_priority);
        unsafe { threshold_ptr.read_volatile() & 7 }
    }
    // 读取 claim 寄存器, 表示服务(宠幸)那个中断源
    pub fn claim(&mut self, hart_id: usize, target_priority: IntrTargetPriority) -> u32 {
        let claim_comp_ptr = self.claim_comp_ptr_of_hart_with_priority(hart_id, target_priority);
        unsafe { claim_comp_ptr.read_volatile() }
    }
    // 写入 claim/complete 寄存器 (同一个寄存器, 读的时候叫claim 写的时候叫complete) 表示服务(宠幸)完毕
    // arg: completion 服务完毕的中断源ID
    pub fn complete(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        completion: u32,
    ) {
        let claim_comp_ptr = self.claim_comp_ptr_of_hart_with_priority(hart_id, target_priority);
        unsafe {
            claim_comp_ptr.write_volatile(completion);
        }
    }
}
