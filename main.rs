use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::collections::VecDeque;
use std::process;


#[derive(Clone)]
struct Process {
    name: String,       // process name
    pid: u32,       // process ID
    ppid: u32,      // parent process ID
    status: String,     // process status: S(sleeping) / W(waiting) / None
    lines: VecDeque<String>,     // process file의 명령어 저장 queue
    virtual_mem: [Option<Page>;32],         // process의 가상 메모리
    page_table: [[i32;2];32],       // page table: (page id, frame id)를 요소로 가짐
    page_id: i32,       // 해당 프로세스에서 마지막으로 할당한 페이지 ID
    allocation_id: i32,         // 해당 프로세스에서 마지막으로 할당한 allocation ID
}

#[derive(Copy, Clone)]
struct Page {
    pid: u32,       // process ID
    page_id: i32,       
    allocation_id: i32,
    authority: i32,         // 페이지 권한 - 0: 읽기,쓰기 모두 가능 / 1: 읽기만 가능
    count: u32,     // 페이지가 사용된 횟수 
}

static mut CYCLE: u32 = 0;
static mut PID: u32 = 1;
static mut MODE: String = String::new();       // user or kernel
static mut COMMAND: String = String::new();
static mut RQ: VecDeque<Process> = VecDeque::new();       // ready queue
static mut WQ: VecDeque<Process> = VecDeque::new();       // waiting queue
static mut RUNNING: Option<Process> = None;        // 현재 실행 중인 process
static mut NEWP: Option<Process> = None;       // 새로 들어온 process 
static mut TERMINATED: Option<Process> = None;     // terminated 상태인 process 
static NONE_PAGE: Option<Page> = None;
static mut PHYSICAL_MEM: [Option<Page>;16] = [NONE_PAGE;16];        // 물리 메모리
static mut PQ: VecDeque<Page> = VecDeque::new();        // 물리 메모리에 들어오는 페이지 순서대로 저장하는 큐(FIFO, LRU일 때만 사용, LRU일 경우는 추가적으로 페이지가 사용될 때마다 큐 업데이트)
static mut CYCLE_INFO: String = String::new();      // result 파일에 출력할 cycle 정보
static mut CYCLE_DONE: bool = false;     // cycle이 끝나고 결과를 출력해야하면 true / 아직 출력할 때가 아니면 false
static mut INPUT_DIR: String = String::new();       // 가상 프로그램들이 들어있는 폴더 경로 저장
static mut CHANGE_ALGORITHM: String = String::new();        // 페이지 교체 알고리즘 저장

// 새로운 process 만들고 return하는 함수
fn create_process(name: String, pid: u32, ppid: u32, status: String, lines: VecDeque<String>, virtual_mem: [Option<Page>;32], page_table: [[i32;2];32], page_id: i32, allocation_id: i32) -> Process {
    Process {
        name,
        pid,
        ppid,     
        status,
        lines,
        virtual_mem,
        page_table,
        page_id,
        allocation_id,
    }
}

// 새로운 page 만들고 return하는 함수
fn create_page(pid: u32, page_id: i32, allocation_id: i32, authority: i32, count: u32) -> Page {
    Page {
        pid,
        page_id,
        allocation_id,
        authority,
        count,
    }
}

// 매 cycle에 관한 정보 CYCLE_INFO에 추가하는 함수
fn print_cycle()
{
    unsafe{
        let mut temp: String;
        if !CYCLE_DONE {return;}
        else
        {
            temp = format!("[cycle #{CYCLE}]\n1. mode: {MODE}\n2. command: {COMMAND}\n");
            // 3. running 출력
            match &RUNNING {
                None => temp.push_str("3. running: none\n"),
                Some(p) => temp.push_str(&format!("3. running: {}({}, {})\n", p.pid, p.name, p.ppid))
            }
            // 4. physical memory 출력
            temp.push_str("4. physical memory: \n");
            let mut temp2 = "|".to_string();
            for i in 0..16 {
                match PHYSICAL_MEM[i] {
                    None => {
                        if i%4==3 {
                            temp2.push_str("-|");
                        }
                        else {
                            temp2.push_str("- ");
                        }
                    }
                    Some(p) => {
                        if i%4==3 {
                            temp2.push_str(&format!("{}({})|", p.pid, p.page_id));
                        }
                        else {
                            temp2.push_str(&format!("{}({}) ", p.pid, p.page_id));
                        }
                    }
                }
            }
            temp.push_str(&format!("{}\n", temp2));
            
            // 5. virtual memory, 6. page table 출력 (running process 존재 시)
            match &RUNNING {
                None => temp.push_str("\n"),        // running process가 없을 경우 5, 6 출력 X
                Some(p) => {
                    let mut temp2 = "|".to_string();
                    let mut temp3 = "|".to_string();
                    let mut temp4 = "|".to_string();
                    temp.push_str("5. virtual memory: \n");
                    for i in 0..32 {
                        match p.virtual_mem[i] {       // running process의 가상 메모리 상황
                            None => {
                                if i%4==3 {
                                    temp2.push_str("-|");
                                    temp4.push_str("-|");
                                }
                                else {
                                    temp2.push_str("- ");
                                    temp4.push_str("- ");
                                }
                            }
                            Some(q) => {
                                if i%4==3 {
                                    temp2.push_str(&format!("{}|", q.page_id));
                                    if q.authority == 0 {
                                        temp4.push_str("W|");
                                    }
                                    else {
                                        temp4.push_str("R|");
                                    }
                                }
                                else {
                                    temp2.push_str(&format!("{} ", q.page_id));
                                    if q.authority == 0 {
                                        temp4.push_str("W ");
                                    }
                                    else {
                                        temp4.push_str("R ");
                                    }
                                }
                            }
                        }
                        if p.page_table[i][1] == -1 {
                            if i%4==3 {
                                temp3.push_str("-|");
                            }
                            else {
                                temp3.push_str("- ");
                            }
                        }
                        else {
                            if i%4==3 {
                                temp3.push_str(&format!("{}|", p.page_table[i][1]));
                            }
                            else {
                                temp3.push_str(&format!("{} ", p.page_table[i][1]));
                            }
                        }
                    }
                    temp.push_str(&format!("{}\n6. page table: \n", temp2));
                    temp.push_str(&format!("{}\n{}\n\n", temp3, temp4));
                }
            }
        }
        CYCLE_INFO.push_str(&temp);
    }
}

// schedule 함수
fn idle_or_schedule()
{
    unsafe{
        MODE = String::from("kernel");
        CYCLE += 1;     // 1 cycle 소비
        if !RUNNING.is_none() {return;}     // 이미 running 상태의 process가 있다면 스케줄 필요X
        else {
            match RQ.pop_front() {
                None => {
                    COMMAND = String::from("idle");     // ready queue is empty
                    CYCLE_DONE = true;
                    print_cycle();
                    return;
                }
                Some(p) => {
                    COMMAND = String::from("schedule");
                    RUNNING = Some(p);       // ready queue의 첫번째 process를 running으로
                    CYCLE_DONE = true;
                    print_cycle();
                    running_process();      // 다음 프로세스 진행
                }
            }
        }
    }
}

// 현재 running process의 가상 메모리에서 arg1 만큼의 빈 공간을 찾는 함수 -> 빈 공간의 첫 번째 index 반환
fn find_virtual_space(arg1: u32) -> u32 {
    unsafe {
        let mut result: u32 = 32;
        if arg1 > 16 {return result;}
        match &RUNNING {
            None => result,
            Some(r) => {
                for i in 0..(33-arg1) {
                    if r.virtual_mem[i as usize].is_none() {
                        let mut check: bool = true;
                        for j in 0..arg1 {
                            if !r.virtual_mem[(i+j) as usize].is_none() {
                                check = false;
                                break;
                            }
                            else {continue;}
                        }
                        if check {
                            result = i as u32;
                            break;
                        }
                    }
                    else {continue;}
                }
                result
            }
        }
    }
}

// 물리 메모리에서 arg1개 만큼의 빈 공간을 찾아 index를 배열에 넣어 반환하는 함수 
fn find_physical_space(arg1: u32) -> Vec<u32> {
    let mut result:Vec<u32> = Vec::new();
    unsafe {
        // 물리 메모리에서 비어있는 공간 위치 저장하기
        for i in 0..16 {
            if PHYSICAL_MEM[i].is_none() {
                result.push(i as u32);
                if result.len() == arg1 as usize {break;}
            }
            else {continue;}
        }

        // 물리메모리에서 페이지 교체가 필요할 경우
        while result.len() < arg1 as usize {
            let victim:u32 = find_victim();     // 교체될 페이지
            result.push(victim);
        }
    }
    result.sort();
    result      // pop을 하면 상위 index부터 튀어나오므로, v.get(0)부터 접근할 것
}

// 페이지 교체 알고리즘에 맞게 victim 페이지 찾고 물리 메모리에서 해제 & 페이지 테이블 업데이트 & 비워진 공간의 index 반환
fn find_victim() -> u32{
    unsafe {
        let mut victim: Option<Page> = None;
        let mut index: u32 = 16;
        if CHANGE_ALGORITHM.contains("fifo") || CHANGE_ALGORITHM.contains("lru") {
            victim = PQ.pop_front();
        }
        else if CHANGE_ALGORITHM.contains("lfu") {
            let mut min = -1; 
            for i in 0..16 {
                if !PHYSICAL_MEM[i].is_none() {
                    let page = PHYSICAL_MEM[i].unwrap();
                    if min == -1 || min > page.count as i32{
                        min = page.count as i32;
                        victim = PHYSICAL_MEM[i];
                        index = i as u32;
                    }
                }
            }
        }
        else {      // 페이지 교체 알고리즘: MFU
            let mut max = -1; 
            for i in 0..16 {
                if !PHYSICAL_MEM[i].is_none() {
                    let page = PHYSICAL_MEM[i].unwrap();
                    if max < page.count as i32{
                        max = page.count as i32;
                        victim = PHYSICAL_MEM[i];
                        index = i as u32;
                    }
                }
            }
        }
        // fifo 또는 lru일 경우
        if CHANGE_ALGORITHM.contains("fifo") || CHANGE_ALGORITHM.contains("lru") {
            // 1. victim page가 위치한 물리메모리에서의 index 구하기
            for i in 0..16 {
                if !PHYSICAL_MEM[i].is_none() {
                    let p = PHYSICAL_MEM[i].unwrap();
                    if p.pid == victim.unwrap().pid && p.page_id == victim.unwrap().page_id {
                        index = i as u32;
                        break;
                    }
                }
            }
        }
        // victim page를 가지는 모든 프로세스들에게서 페이지 테이블 업데이트
            // 1. running process 탐색
        match &RUNNING {
            None => return index,
            Some(r) => {
                for i in 0..32 {
                    if r.virtual_mem[i].is_none() {continue;}
                    let target = r.virtual_mem[i].unwrap();
                    if target.pid == victim.unwrap().pid && target.page_id == victim.unwrap().page_id {
                        let mut c:Process = r.clone();
                        c.page_table[i][1] = -1;
                        RUNNING = Some(c);
                        break;
                    }
                }
            }
        }
            // 2. ready queue 탐색 
        let size = RQ.len();
        for _ in 0..size {
            let mut p: Process = RQ.pop_front().unwrap();
            for i in 0..32 {
                if p.virtual_mem[i].is_none() {continue;}
                let target = p.virtual_mem[i].unwrap();
                if target.pid == victim.unwrap().pid && target.page_id == victim.unwrap().page_id {
                    p.page_table[i][1] = -1;
                    break;
                }
            }
            RQ.push_back(p);
        }
            // 3. waiting queue 탐색
        let size = WQ.len();
        for _ in 0..size {
            let mut p: Process = WQ.pop_front().unwrap();
            for i in 0..32 {
                if p.virtual_mem[i].is_none() {continue;}
                let target = p.virtual_mem[i].unwrap();
                if target.pid == victim.unwrap().pid && target.page_id == victim.unwrap().page_id {
                    p.page_table[i][1] = -1;
                    break;
                }
            }
            WQ.push_back(p);
        }
        PHYSICAL_MEM[index as usize] = None;     // victim 자리 비우기
        index
    }
}

// 명령어 memory_allocate 처리 함수
fn memory_allocate(arg1: u32) {
    unsafe {
        // 1. 첫 번째 cycle 출력 
        CYCLE += 1;
        COMMAND = String::from(format!("memory_allocate {arg1}"));
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // 모드 스위칭

        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        COMMAND = String::from("system call");
        let virtual_index = find_virtual_space(arg1);
        let physical_index: Vec<u32> = find_physical_space(arg1);
        match &RUNNING {
            None => return,
            Some(r) => {
                let mut c = r.clone();
                c.allocation_id += 1;
                for i in 0..arg1 {
                    c.page_id += 1;
                    let new_page:Page = create_page(c.pid, c.page_id, c.allocation_id, 0, 1);     // 새로운 페이지 생성
                    let index_v = i+virtual_index;
                    let &index_p: &u32 = physical_index.get(i as usize).unwrap();
                    c.virtual_mem[index_v as usize] = Some(new_page);        // 가상메모리에 할당
                    PHYSICAL_MEM[index_p as usize] = Some(new_page);       // 물리메모리에 할당
                    if CHANGE_ALGORITHM.contains("fifo") || CHANGE_ALGORITHM.contains("lru") {
                        PQ.push_back(new_page);
                    }
                    c.page_table[index_v as usize][0] = new_page.page_id;
                    c.page_table[index_v as usize][1] = index_p as i32;       // 새롭게 할당된 페이지에 대해 페이지 테이블 업데이트
                }
                RQ.push_back(c);        // running -> ready
                RUNNING = None;
            }
        }
        CYCLE_DONE = true;
        print_cycle();

        // 3. 세 번째 cycle
        idle_or_schedule();     // scheduling
        return;
    }
}

// ready queue, waiting queue에서 특정 페이지 ID의 페이지를 갖는 모든 프로세스에 대해 페이지 권한 W로 변경,
// 자식 프로세스의 경우 물리메모리를 가리키지 않게 하는 함수 
fn rq_wq_search_and_w_change(page_pid: u32, page_id: u32) {
    unsafe {
        // 1) ready queue에서 해당 페이지를 갖는 프로세스(부모 or 자식) 찾기 
        let mut size = RQ.len();
        for _ in 0..size {
            let mut target = RQ.pop_front().unwrap();
            for i in 0..32 {
                if !target.virtual_mem[i].is_none() {
                    let old_page = target.virtual_mem[i].unwrap();
                    if old_page.pid == page_pid && old_page.page_id == page_id as i32 {
                        let mut new_page = authority_change(old_page, 1);       // 해당 페이지 권한 W로 변경
                        if target.pid != page_pid {     // 자식 프로세스일 경우
                            new_page.pid = target.pid;      // 해당 페이지 부모로부터 독립
                            target.page_table[i][1] = -1;       // 자식 프로세스는 물리메모리를 가리키지 않게 함
                        }
                        target.virtual_mem[i] = Some(new_page);
                        break;
                    }
                }
            }
            RQ.push_back(target);
        }
        // 2) waiting queue에서 해당 페이지를 갖는 프로세스(부모 or 자식) 찾기 
        size = WQ.len();
        for _ in 0..size {
            let mut target = WQ.pop_front().unwrap();
            for i in 0..32 {
                if !target.virtual_mem[i].is_none() {
                    let old_page = target.virtual_mem[i].unwrap();
                    if old_page.pid == page_pid && old_page.page_id == page_id as i32 {
                        let mut new_page = authority_change(old_page, 1);       // 해당 페이지 권한 W로 변경
                        target.virtual_mem[i] = Some(new_page);
                        if target.pid != page_pid {     // 자식 프로세스일 경우
                            new_page.pid = target.pid;      // 해당 페이지 부모로부터 독립
                            target.page_table[i][1] = -1;       // 자식 프로세스는 물리메모리를 가리키지 않게 함
                        }
                        target.virtual_mem[i] = Some(new_page);
                        break;
                    }
                }
            }
            WQ.push_back(target);
        }
    }
}

// PQ에서 해당 페이지 삭제
fn remove_PQ(victim: Page) {
    unsafe {
        let size = PQ.len();
        for _ in 0..size {
            let p = PQ.pop_front().unwrap();
            if p.pid == victim.pid && p.page_id == victim.page_id {continue;}       // victim 페이지를 찾았다면 pop
            else {PQ.push_back(p);}     // victim이 아닌 페이지들은 다시 push
        }
    }
}

fn release(allocation_id: u32) {
    unsafe {
        match &RUNNING {
            None => return,
            Some(r) => {
                let mut c = r.clone();
                for i in 0..32 {
                    if c.virtual_mem[i].is_none() {continue;}
                    let page = c.virtual_mem[i].unwrap();
                    if page.allocation_id == allocation_id as i32 { 
                        let p_index = c.page_table[i][1];       // 물리메모리에서의 위치 
                        c.virtual_mem[i] = None;        // 가상메모리 해제
                        c.page_table[i][0] = -1;
                        c.page_table[i][1] = -1;    // 페이지 테이블 업데이트
                        if page.authority == 0 {        // 해당 페이지 권한이 W일 경우
                            if p_index != -1 {
                                if CHANGE_ALGORITHM.contains("fifo") || CHANGE_ALGORITHM.contains("lru") {
                                    remove_PQ(PHYSICAL_MEM[p_index as usize].unwrap());
                                }
                                PHYSICAL_MEM[p_index as usize] = None;  // 물리메모리에 존재 시 해제
                            }
                        } else {        // 해당 페이지 권한이 R일 경우
                            rq_wq_search_and_w_change(page.pid, page.page_id as u32);       // 해당 페이지를 가지는 모든 부모, 자식, 형제 프로세스에서 권한 W로 변경
                            if page.pid == c.pid {      // running process가 부모일 경우
                                if p_index != -1 {
                                    if CHANGE_ALGORITHM.contains("fifo") || CHANGE_ALGORITHM.contains("lru") {
                                        remove_PQ(PHYSICAL_MEM[p_index as usize].unwrap());
                                    }
                                    PHYSICAL_MEM[p_index as usize] = None;  // 물리메모리에 존재 시 해제
                                }
                            }
                        }    
                    }
                }
                RUNNING = Some(c);
            }
        }
    }
}

// 명령어 memory_release 처리 
fn memory_release(arg1: u32) {
    unsafe{
        // 1. 첫 번째 cycle 출력 
        CYCLE += 1;
        COMMAND = String::from(format!("memory_release {arg1}"));
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // 모드 스위칭

        // 2. 두 번째 cycle 
        CYCLE += 1;
        COMMAND = String::from("system call");
        release(arg1);
        match &RUNNING {
            None => return,
            Some(r) => {
                RQ.push_back(r.clone());
                RUNNING = None;     // running process는 ready 상태가 됨
            }
        }
        CYCLE_DONE = true;
        print_cycle();

        // 3. 세 번째 cycle
        idle_or_schedule();     // scheduling
        return;
    }
}

// 인자로 페이지를 받고, 페이지의 권한만 W에서 R 또는 R에서 W로 변경해서 새 페이지를 반환하는 함수
fn authority_change(old_page: Page, authority: u32) -> Page {
    if authority == 0 {
        return create_page(old_page.pid, old_page.page_id, old_page.allocation_id, 1, old_page.count);
    }
    return create_page(old_page.pid, old_page.page_id, old_page.allocation_id, 0, old_page.count);
}

// 인자로 물리메모리에서의 인덱스를 받고, 해당 위치 페이지의 참조 카운트 +1 하기
fn p_mem_count_plus(p_index: u32) {
    unsafe {
        let old_page = PHYSICAL_MEM[p_index as usize].unwrap();
        let new_page = create_page(old_page.pid, old_page.page_id, old_page.allocation_id, old_page.authority, old_page.count+1);
        PHYSICAL_MEM[p_index as usize] = Some(new_page);
    }
}

// 명령어 memory_read 처리
fn memory_read(arg1: u32) {
    unsafe {
        // 1. 첫 번째 cycle: 읽기 시도
        CYCLE += 1;
        COMMAND = String::from(format!("memory_read {arg1}"));
        // 물리메모리에 해당 페이지가 존재하는지 알아보기
        let mut p_index = -1;
        let mut target = None;      // read하고자 하는 페이지 저장
        match &RUNNING {
            None => return,
            Some(r) => {
                for i in 0..32 {
                    if !r.virtual_mem[i].is_none() && r.virtual_mem[i].unwrap().page_id == arg1 as i32 {
                        p_index = r.page_table[i][1];   // 물리메모리에서의 위치
                        target = r.virtual_mem[i];      
                        break;
                    }
                }
            }
        }
        if p_index != -1 {
            p_mem_count_plus(p_index as u32);     // 참조 카운트 +1
            if CHANGE_ALGORITHM.contains("lru") {      // 페이지 교체 알고리즘이 lru일 경우 PQ 업데이트
                lru_update(PHYSICAL_MEM[p_index as usize].unwrap());
            }
            CYCLE_DONE = true;
            print_cycle();
            running_process();      // 다음 프로세스 명령어 실행
            return;
        }
        // 물리메모리에 존재하지 않을 경우
        CYCLE_DONE = true;
        print_cycle();
        // 2. 두 번째 cycle: 페이지 폴트 핸들러
        CYCLE += 1;
        COMMAND = String::from("fault");
        MODE = String::from("kernel");      // 모드 스위칭
        p_index = page_fault_handler(arg1) as i32;
        // 물리메모리에 새로 할당 후 페이지 테이블 업데이트
        page_table_frame_add(target.unwrap(), p_index as u32);
        match &RUNNING {
            None => return,
            Some(r) => {
                RQ.push_back(r.clone());
                RUNNING = None;     // running -> ready
            }
        }
        CYCLE_DONE = true;
        print_cycle();
        // 3. 세 번째 cycle
        idle_or_schedule();     // scheduling
        return;
    }
}

// 해당 페이지를 갖는 페이지 테이블 업데이트 -> arg1이 page ID인 곳에 새로운 물리메모리 인덱스 (idx) 넣기
fn page_table_frame_add(target: Page, idx: u32) {
    unsafe {
        // 1. running process 탐색 
        match &RUNNING {
            None => return,
            Some(r) => {
                let mut c = r.clone();
                for i in 0..32 {
                    if c.virtual_mem[i].is_none() {continue;}
                    let page = c.virtual_mem[i].unwrap();
                    if page.pid == target.pid && page.page_id == target.page_id {
                        c.page_table[i][1] = idx as i32;
                        break;
                    }
                }
                RUNNING = Some(c);
            }
        }
        // 2. ready queue 탐색
        let mut size = RQ.len();
        for _ in 0..size {
            let mut process = RQ.pop_front().unwrap();
            for i in 0..32 {
                if process.virtual_mem[i].is_none() {continue;}
                let page = process.virtual_mem[i].unwrap();
                if page.pid == target.pid && page.page_id == target.page_id {
                    process.page_table[i][1] = idx as i32;
                    break;
                }
            }
            RQ.push_back(process);
        }
        // 3. waiting queue 탐색
        size = WQ.len();
        for _ in 0..size {
            let mut process = WQ.pop_front().unwrap();
            for i in 0..32 {
                if process.virtual_mem[i].is_none() {continue;}
                let page = process.virtual_mem[i].unwrap();
                if page.pid == target.pid && page.page_id == target.page_id {
                    process.page_table[i][1] = idx as i32;
                    break;
                }
            }
            WQ.push_back(process);
        }
    }
}

// 명령어 memory_write 처리 함수
fn memory_write(arg1: u32) {
    unsafe {
        // 1. 첫 번째 cycle : 유저 모드
        CYCLE += 1;
        COMMAND = String::from(format!("memory_write {arg1}"));
        CYCLE_DONE = true;
        print_cycle();
        
        let mut p_index = -1;
        let mut autho = 0;
        let mut page = None;   
        let mut running_pid = 0;
        let mut v_index = 0;
        match &RUNNING {
            None => return,
            Some(r) => {
                let mut c = r.clone();
                running_pid = c.pid;
                for i in 0..32 {
                    if c.virtual_mem[i].is_none() {continue;}
                    let target = c.virtual_mem[i].unwrap();
                    if target.page_id == arg1 as i32 {
                        p_index = c.page_table[i][1];
                        v_index = i;
                        autho = target.authority;
                        if autho == 1 {     // 권한이 R이었을 경우
                            c.virtual_mem[i] = Some(authority_change(target, 1));       // 권한 W로 변경
                        }
                        page = c.virtual_mem[i];
                        break;
                    }
                }
                RUNNING = Some(c);
            }
        }

        if autho == 0 {     // 권한이 W였을 경우
            if p_index != -1 {      // 물리메모리에 있는 경우
                p_mem_count_plus(p_index as u32);
                if CHANGE_ALGORITHM.contains("lru") {      // 페이지 교체 알고리즘이 lru일 경우 PQ 업데이트
                    lru_update(PHYSICAL_MEM[p_index as usize].unwrap());
                }
                running_process();      // 다음 유저 명령어 실행
            } else {        // 물리메모리에 없는 경우
                // 2. 두 번째 cycle : page fault handle
                CYCLE += 1;
                COMMAND = String::from("fault");
                MODE = String::from("kernel");
                p_index = page_fault_handler(arg1) as i32;     // 물리메모리에 새롭게 할당
                page_table_frame_add(page.unwrap(), p_index as u32);        // 페이지 테이블 업데이트
                match &RUNNING {
                    None => return,
                    Some(r) => {
                        RQ.push_back(r.clone());    
                        RUNNING = None;     // running -> ready
                    }
                }
                CYCLE_DONE = true;
                print_cycle();
                // 3. 세 번째 cycle : scheduling 
                idle_or_schedule();
                return;
            }
        } else {        // 권한이 R이었을 경우
            CYCLE += 1;
            COMMAND = String::from("fault");
            MODE = String::from("kernel");
            rq_wq_search_and_w_change(page.unwrap().pid, page.unwrap().page_id as u32);
            if p_index != -1 {
                PHYSICAL_MEM[p_index as usize] = page;      // 물리메모리에서 기존의 프레임 권한 W로 변경
            }
            if page.unwrap().pid != running_pid {       // running process가 자식일 경우
                let new_page = create_page(running_pid, page.unwrap().page_id, page.unwrap().allocation_id, 0, 1);
                match &RUNNING {
                    None => return, 
                    Some(r) => {
                        let mut c = r.clone();
                        c.virtual_mem[v_index as usize] = Some(new_page);
                        RUNNING = Some(c);
                    }
                }
                p_index = page_fault_handler(arg1) as i32;     // 물리메모리에 새롭게 할당
                page_table_frame_add(new_page, p_index as u32);        // 페이지 테이블 업데이트
            } else {        // running process가 부모일 경우
                if p_index != -1{
                    p_mem_count_plus(p_index as u32);       // 물리메모리에 존재하면 참조 카운트 +1
                    if CHANGE_ALGORITHM.contains("lru") {      // 페이지 교체 알고리즘이 lru일 경우 PQ 업데이트
                        lru_update(PHYSICAL_MEM[p_index as usize].unwrap());
                    }
                } else {        // 물리메모리에 존재하지 않을 시
                    p_index = page_fault_handler(arg1) as i32;     // 물리메모리에 새롭게 할당
                    page_table_frame_add(page.unwrap(), p_index as u32);        // 페이지 테이블 업데이트
                }
            }
            match &RUNNING {
                None => return,
                Some(r) => {
                    RQ.push_back(r.clone());    
                    RUNNING = None;     // running -> ready
                }
            }
            CYCLE_DONE = true;
            print_cycle();
            // 3. 세 번째 cycle : scheduling 
            idle_or_schedule();
            return;
        }
    }
}

// 페이지 교체 알고리즘이 LRU일 경우 참조되는 페이지를 인자로 받고 이를 PQ에서 맨 위로 업데이트하는 함수
fn lru_update(target: Page) {
    unsafe {
        let size = PQ.len();
        let mut top: Option<Page> = None;
        for _ in 0..size {
            let page = PQ.pop_front().unwrap();
            if page.pid == target.pid && page.page_id == target.page_id {
                top = Some(page);
            }
            else {PQ.push_back(page);}
        }
        PQ.push_back(top.unwrap());
    }
}

// 필요한 페이지의 page id를 인자로 받고 이를 물리 메모리에 할당하는 함수 -> 물리메모리에서의 인덱스 반환
fn page_fault_handler(page_id: u32) -> u32 {
    unsafe {
        MODE = String::from("kernel");
        let p_index = find_physical_space(1).pop().unwrap();       // 필요한 페이지를 할당할 물리 메모리에서의 index
        match &RUNNING {
            None => return 0,
            Some(r) => {
                for i in 0..32 {
                    if r.virtual_mem[i].is_none() {continue;}
                    if r.virtual_mem[i].unwrap().page_id == page_id as i32 {
                        let new_page = create_page(r.virtual_mem[i].unwrap().pid, page_id as i32, r.virtual_mem[i].unwrap().allocation_id, r.virtual_mem[i].unwrap().authority, 1);
                        PHYSICAL_MEM[p_index as usize] = Some(new_page);     
                        if CHANGE_ALGORITHM.contains("fifo") ||  CHANGE_ALGORITHM.contains("lru") {
                            PQ.push_back(new_page);
                        }
                        break;
                    }
                }
            }
        }
        return p_index;
    }
}

// 명령어 fork_and_exec 처리 
fn fork_and_exec(name: String) {
    unsafe{
        // 1. fork 명령어가 실행된 첫 번째 cycle 출력 
        CYCLE += 1;
        COMMAND = String::from(format!("fork_and_exec {name}"));
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");
        
        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        COMMAND = String::from("system call");
        match &RUNNING {
            None => return,
            Some(r) => {
                // 새로운 process 생성
                    // 새로 들어온 process를 읽고 한 줄씩 VecDeque에 저장
                let process_dir: String = format!("{}\\{}", INPUT_DIR, name).to_string();       
                let mut lines: VecDeque<String> = VecDeque::new();
                let file = File::open(process_dir).unwrap();
                let reader = BufReader::new(file).lines();
                for line in reader {
                    lines.push_back(line.unwrap());
                }
                PID += 1;
                let mut new_r = r.clone();
                // running process의 페이지 모두 권한을 W -> R로 (물리메모리도 수정)
                for i in 0..32 {
                    if !new_r.virtual_mem[i].is_none() {
                        new_r.virtual_mem[i] = Some(authority_change(new_r.virtual_mem[i].unwrap(), 0));     // R로 권한 변경
                        if new_r.page_table[i][1] != -1 {
                            let old_page = PHYSICAL_MEM[new_r.page_table[i][1] as usize].unwrap();
                            PHYSICAL_MEM[new_r.page_table[i][1] as usize] = Some(authority_change(old_page, 0));
                        }
                    }
                    else {continue;}
                }
                // 부모 프로세스의 가상 메모리를 CoW
                let p = create_process(name, PID, new_r.pid, "None".to_string(), lines, new_r.virtual_mem, new_r.page_table, new_r.page_id, new_r.allocation_id);
                NEWP = Some(p);     // new process 갱신
                RQ.push_back(new_r.clone());      // 부모 process(현재 running process) ready queue에 넣기
                RUNNING = None;
            }
        }
        CYCLE_DONE = true;
        print_cycle();

        // 3. 세 번째 cycle 출력
            // new 상태의 process ready queue에 넣기
        match &NEWP {
            None => return,
            Some(p) => {
                RQ.push_back(p.clone());
                NEWP = None;
            }
        } 
        idle_or_schedule();     // scheduling
        return;
    }
}

// 명령어 wait 처리
fn wait() {
    unsafe{
        // 1. 첫 번째 cycle 출력
        CYCLE += 1;
        COMMAND = String::from("wait");
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // 모드 스위칭

        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        COMMAND = String::from("system call");
            // ready queue에 자식 프로세스가 존재하는지 확인
        let mut find = false;
        match &RUNNING {
            None => return,
            Some(p) => {
                for (_, value) in RQ.iter_mut().enumerate() {
                    if value.ppid == p.pid {      // 자식 프로세스 존재
                        let mut p1 = p.clone();
                        p1.status = "W".to_string();
                        WQ.push_back(p1);
                        find = true;
                        break;
                    }
                }
                if !find {      // 자식 프로세스 없음
                    RQ.push_back(p.clone());
                }
                RUNNING = None;
                CYCLE_DONE = true;
                print_cycle();
            }
        }

        // 3. 세 번째 cycle 출력 
        idle_or_schedule();
    }
}

// 명령어 exit 처리
fn exit() {
    unsafe{
        // 1. 첫 번째 cycle 출력
        CYCLE += 1;
        COMMAND = String::from("exit");
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // 모드 스위칭

        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        COMMAND = String::from("system call");
        match &RUNNING {
            None => return,
            Some(c) => {
                // 부모 process가 waiting 중인지 확인
                for (index, value) in WQ.iter_mut().enumerate() {
                    if value.pid == c.ppid {
                        RQ.push_back(value.clone());
                        WQ.remove(index);
                        break;
                    }
                }
                    // 해당 프로세스의 모든 allocation id에 대해 release
                let mut al:Vec<u32> = Vec::new();
                for i in 0..32 {
                    if !c.virtual_mem[i].is_none() {
                        let a = c.virtual_mem[i].unwrap().allocation_id as u32;
                        if !al.contains(&a) {al.push(a);}
                    }
                }
                let size = al.len();
                for _ in 0..size {
                    release(al.pop().unwrap());
                }
                TERMINATED = Some(c.clone());
                RUNNING = None;
            }
        }
        CYCLE_DONE = true;
        print_cycle();

        // 3. 세 번째 cycle 출력
        TERMINATED = None;
        match &NEWP {
            Some(_) => {            // 종료되지 않은 new process가 존재할 경우
                idle_or_schedule();      
            },
            None => {
                if RQ.is_empty() && WQ.is_empty() {     // 종료되지 않은 프로세스가 running process 단 하나일 경우
                    return;
                } else {        // 종료되지 않은 프로세스가 더 남아있는 경우
                    idle_or_schedule();
                }
            }
        }
    }
}

// 프로그램 파일 읽고 명령어에 맞게 처리하는 함수
fn running_process() {
    unsafe{
        match &RUNNING {
            None => return,
            Some(p) => {
                let mut v = p.lines.clone();
                while !v.is_empty() {
                    MODE = String::from("user");
                    let order = v.pop_front().unwrap();
                    if order.contains("exit") {      // 명령어 exit가 들어왔을 경우
                        exit();
                        return;
                    }
                    // running process의 요소 lines를 이후 남은 명령어들의 queue로 갱신해주기
                    let mut new_lines: VecDeque<String> = VecDeque::new();
                    for after in &v {
                        new_lines.push_back(after.to_string());
                    }
                    let c = p.clone();
                    RUNNING = Some(create_process(c.name, c.pid, c.ppid, "None".to_string(), new_lines, c.virtual_mem, c.page_table, c.page_id, c.allocation_id));
                    if order.contains("memory_allocate") {      // 명령어 memory_allocate이 들어왔을 경우
                        let n: u32 = order.trim().split(" ").last().unwrap().parse().unwrap();
                        memory_allocate(n);
                        return;
                    } else if order.contains("memory_release") {        // 명령어 memory_release가 들어왔을 경우
                        let n: u32 = order.trim().split(" ").last().unwrap().parse().unwrap();
                        memory_release(n);
                        return;
                    } else if order.contains("memory_read") {      // 명령어 memory_read가 들어왔을 경우
                        let n: u32 = order.trim().split(" ").last().unwrap().parse().unwrap();
                        memory_read(n);
                        return;
                    } else if order.contains("memory_write") {      // 명령어 memory_write가 들어왔을 경우
                        let n: u32 = order.trim().split(" ").last().unwrap().parse().unwrap();
                        memory_write(n);
                        return;
                    } else if order.contains("fork_and_exec") {       // 명령어 fork가 들어왔을 경우
                        let name = order.trim().split(" ").last().unwrap().to_string();
                        fork_and_exec(name);
                        return;
                    } else if order.contains("wait") {      // 명령어 wait가 들어왔을 경우
                        wait();
                        return;
                    } else {
                        println!("wrong order!");
                        return;
                    }
                }
            }
        }
    }
}

fn main() {     
    unsafe{
        let args: Vec<String> = env::args().collect();
        INPUT_DIR = String::from(&args[1]);    // input file들이 있는 폴더 경로 저장 
        CHANGE_ALGORITHM = String::from(&args[2]);      // 페이지 교체 알고리즘 저장

        // cycle #0
        // init 생성 
        let process_dir: String = format!("{}\\{}", INPUT_DIR, "init").to_string();    
        let mut lines: VecDeque<String> = VecDeque::new();
        let file = File::open(process_dir).unwrap();
        let reader = BufReader::new(file).lines();
        for line in reader {
            lines.push_back(line.unwrap());     // int 프로그램 파일 읽고 한 줄씩 VecDeque에 저장
        }
        let virtual_mem: [Option<Page>;32] = [NONE_PAGE;32];    
        let page_table: [[i32;2];32] = [[-1;2];32];     
        let init = create_process("init".to_string(), PID, 0, "None".to_string(), lines, virtual_mem, page_table, -1, -1);
        MODE = String::from("kernel");
        COMMAND = String::from("boot");
        NEWP = Some(init);
        CYCLE_DONE = true;
        print_cycle();

        // cycle #1
            // new process -> ready queue
        match &NEWP {
            None => return,
            Some(p) => {
                RQ.push_back(p.clone());
                NEWP = None;
            }
        }
        idle_or_schedule();     // ready -> running

        // cycle #2~ 
        running_process();

        let mut result = std::fs::File::create("result").expect("create failed");
        result.write_all(CYCLE_INFO.as_bytes()).expect("write failed");
        println!("result written to file" );
        process::exit(1);
    }
}
