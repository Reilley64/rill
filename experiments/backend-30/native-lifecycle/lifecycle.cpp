// Standalone fixed-stack migration/loan-retention mechanism, not a Rill runtime.
#ifdef NDEBUG
#error Assertions must remain enabled
#endif
#include <atomic>
#include <cassert>
#include <cfenv>
#include <condition_variable>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <mutex>
#include <new>
#include <thread>
#include <sys/mman.h>
#include <unistd.h>

// Plain C assembly ABI, as wrapped by pinned boostorg/context/src/fcontext.cpp.
// The harness links only make/jump assembly, not Boost's C++ fiber destructor.
struct Transfer { void *context; void *data; };
extern "C" void *make_fcontext(void *,std::size_t,void (*)(Transfer));
extern "C" Transfer jump_fcontext(void *,void *);
struct Monitor {
  std::atomic<unsigned> borrowers{0}, releases{0}, disposers{0};
  std::atomic<unsigned> clock{0}, native_access_ended{0}, disposed_at{0}, released_at{0};
};
struct Backing {
  unsigned char *mapping,*low,*top;
  std::size_t page,capacity;
  Monitor *monitor;
  explicit Backing(Monitor *m):monitor(m) {
    page=static_cast<std::size_t>(sysconf(_SC_PAGESIZE));capacity=1024*1024;
    assert(capacity%page==0);
    mapping=static_cast<unsigned char *>(mmap(nullptr,capacity+4*page,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0));
    assert(mapping!=MAP_FAILED);low=mapping+2*page;top=low+capacity;
    assert(mprotect(low-page,capacity+2*page,PROT_READ|PROT_WRITE)==0);
    for(unsigned i=0;i<64;++i){low[-64+static_cast<int>(i)]=0xa5;top[i]=0xa5;}
  }
  ~Backing() {
    assert(monitor->borrowers.load()==0);
    for(unsigned i=0;i<64;++i)assert(low[-64+static_cast<int>(i)]==0xa5 && top[i]==0xa5);
    assert(munmap(mapping,capacity+4*page)==0);
    monitor->released_at=++monitor->clock;++monitor->releases;
  }
};
struct Status { std::atomic<bool> cancel{false};std::atomic<unsigned> completed{0}; };
struct Task {
  std::shared_ptr<Status> status=std::make_shared<Status>();
  void *context=nullptr,*scheduler=nullptr;
  Monitor *monitor;
  std::uintptr_t low,high,slot_address=0;
  volatile std::uint64_t *loan=nullptr,*parent_loan=nullptr;
  unsigned workers=0,resumes=0,migrations=0,last_worker=2,terminate_requested=0,terminated=0;
  std::atomic<unsigned> *parallel_barrier=nullptr;
  bool masked=false,cleanup_pending=false,allow_completion=false,mutable_loan=false;
  bool job_joined=false,ended=false,disposal_waiting=false,disposal_ack=false;
  bool deep_cleanup=false,fail_worklist=false;
  std::uint64_t rendered=0,allocated=0,freed=0,callbacks=0,render_hash=0;
  std::int64_t next_dispose=-1;
  std::uintptr_t frame_min=UINTPTR_MAX,frame_max=0;
  void *last_payload=nullptr; // Observably reachable semantic resource graph, not activation frames.
  double float_result=0;
  std::uint64_t admission_need=0,ordinary_entered=0,ordinary_completed=0,least_remaining=UINT64_MAX;
  unsigned recursion_request=0;bool recursion_succeeded=false,capacity_failed=false,join_waiting=false;
};
struct Resume { Task *task; unsigned worker; };
static void arrival(Task *t,Transfer transfer) {
  auto *r=static_cast<Resume *>(transfer.data);
  assert(r->task==t && r->worker<2);t->scheduler=transfer.context;
  if(t->last_worker<2 && t->last_worker!=r->worker)++t->migrations;
  t->last_worker=r->worker;t->workers|=1u<<r->worker;++t->resumes;
}
static void yield(Task *t) { arrival(t,jump_fcontext(t->scheduler,t)); }
[[noreturn]] __attribute__((noinline,not_tail_called)) static void terminate_guest(Task *t) {
  t->terminated=t->terminate_requested;t->status->completed=t->terminated;
  jump_fcontext(t->scheduler,t); // Terminal guest escape, not a source-level result/unwind.
  std::abort();
}
static std::uint64_t admission_need;
extern "C" __attribute__((noinline)) void native_checked_admission(Task *,unsigned);
extern "C" __attribute__((noinline)) void native_ordinary_body(Task *t,unsigned n) {
  alignas(4096) volatile unsigned char frame[8192];
  ++t->ordinary_entered;
  for(unsigned i=0;i<sizeof(frame);++i)frame[i]=static_cast<unsigned char>(i+n);
  if(n)native_checked_admission(t,n-1);
  for(unsigned i=0;i<sizeof(frame);++i)assert(frame[i]==static_cast<unsigned char>(i+n));
  ++t->ordinary_completed; // Post-call live storage makes this ordinary, not eligible-tail work.
}
extern "C" __attribute__((noinline)) void native_checked_admission(Task *t,unsigned n) {
  std::uintptr_t sp;
#if defined(__aarch64__)
  asm volatile("mov %0, sp":"=r"(sp));
#elif defined(__x86_64__)
  asm volatile("mov %%rsp, %0":"=r"(sp));
#else
#error Unsupported fixture architecture
#endif
  auto remaining=sp>=t->low && sp<=t->high?sp-t->low:0;
  if(remaining<t->least_remaining)t->least_remaining=remaining;
  if(sp<t->low || sp>t->high || t->admission_need>remaining) {
    t->capacity_failed=true;t->terminate_requested=3;terminate_guest(t);
  }
  native_ordinary_body(t,n);
}
static std::uint64_t pattern(unsigned i) { return UINT64_C(0x456789abcdef0000)+i; }
struct Guard {
  Task *task;volatile std::uint64_t *slot;
  __attribute__((noinline)) ~Guard() {
    assert(task->job_joined && task->monitor->borrowers.load()==0);
    task->masked=true;
    while(!task->disposal_ack) {task->disposal_waiting=true;yield(task);}
    task->disposal_waiting=false;
    if(task->parent_loan)for(unsigned i=0;i<8;++i)assert(task->parent_loan[i]==pattern(i));
    for (unsigned i=0;i<8;++i)assert(slot[i]==(task->mutable_loan?(pattern(i)^UINT64_C(0xff00)):pattern(i)));
    task->monitor->disposed_at=++task->monitor->clock;
    ++task->monitor->disposers;task->masked=false;
  }
};
__attribute__((noinline,disable_tail_calls)) static void borrowed_scope(Task *t) {
  alignas(64) volatile std::uint64_t slot[8];
  for(unsigned i=0;i<8;++i)slot[i]=pattern(i);
  Guard guard{t,slot};
  t->loan=slot;t->slot_address=reinterpret_cast<std::uintptr_t>(slot);
  assert(t->slot_address>=t->low && t->slot_address+sizeof(slot)<=t->high);
  if(t->parallel_barrier) {
    ++*t->parallel_barrier;
    while(t->parallel_barrier->load()<2)std::atomic_signal_fence(std::memory_order_seq_cst);
  }
  double accumulated=0;
  volatile double increment=0.125;
  while(!t->allow_completion) {
    if(t->terminate_requested)terminate_guest(t);
    if(t->recursion_request) {
      unsigned depth=t->recursion_request;t->recursion_request=0;
      native_checked_admission(t,depth);t->recursion_succeeded=true;
    }
    assert(reinterpret_cast<std::uintptr_t>(slot)==t->slot_address);
    // Exclusive native access freezes owner reads too, not merely concurrent writes.
    if(!t->mutable_loan)for(unsigned i=0;i<8;++i)assert(slot[i]==pattern(i));
    if(t->parent_loan)for(unsigned i=0;i<8;++i)assert(t->parent_loan[i]==pattern(i));
    accumulated+=increment;
    if(t->status->cancel.load()) {t->cleanup_pending=true;t->masked=true;}
    yield(t);
  }
  while(!t->job_joined){t->join_waiting=true;t->masked=true;yield(t);}
  t->join_waiting=false;assert(t->monitor->borrowers.load()==0);
  t->masked=true;t->float_result=accumulated;
}
static void sample_frame(Task *t,const void *p) {
  auto address=reinterpret_cast<std::uintptr_t>(p);assert(address>=t->low && address<t->high);
  if(address<t->frame_min)t->frame_min=address;if(address>t->frame_max)t->frame_max=address;
}
struct Payload {
  Task *task;unsigned id;std::uint64_t *data;Payload *previous;
  Payload(Task *t,unsigned i):task(t),id(i),data(new(std::nothrow) std::uint64_t[2]),previous(static_cast<Payload *>(t->last_payload)) {
    if(!data){t->terminate_requested=3;terminate_guest(t);}
    data[0]=i;data[1]=~static_cast<std::uint64_t>(i);t->last_payload=this;++t->allocated;
  }
  __attribute__((noinline)) ~Payload() {
    unsigned marker=id;sample_frame(task,&marker);
    assert(task->masked && task->next_dispose==id && task->last_payload==this);
    if(id%1024==1){++task->callbacks;yield(task);assert(task->masked && marker==id);}
    assert(data[0]==id && data[1]==~static_cast<std::uint64_t>(id));delete[] data;++task->freed;task->last_payload=previous;
    do{--task->next_dispose;}while(task->next_dispose>=0 && task->next_dispose%3==0);
  }
};
__attribute__((noinline)) static void format_node(Task *t,unsigned i) {
  char text[32];int n=std::snprintf(text,sizeof(text),"%u",i);assert(n>0 && n<32);sample_frame(t,text);
  if(i%4096==0){++t->callbacks;yield(t);assert(t->masked);}
  for(int j=0;j<n;++j)t->render_hash+=static_cast<unsigned char>(text[j]);++t->rendered;
}
static void deep_cleanup(Task *t) {
  constexpr unsigned count=200000;
  struct Node {alignas(Payload) unsigned char storage[sizeof(Payload)];Payload *payload=nullptr;};
  t->masked=true;
  Node **pending=t->fail_worklist?nullptr:new(std::nothrow) Node *[count];
  if(!pending){t->terminate_requested=3;terminate_guest(t);}
  Node *nodes=new(std::nothrow) Node[count];
  if(!nodes){t->terminate_requested=3;terminate_guest(t);}
  for(unsigned i=0;i<count;++i){
    pending[i]=&nodes[i];if(i%3)nodes[i].payload=new(nodes[i].storage) Payload(t,i);
  }
  // Complete outer-to-inner cause-like processing; the formatter callback can
  // suspend with its ordinary local buffer alive. This is not Err<E> itself.
  for(unsigned i=0;i<count;++i)format_node(t,i);
  t->next_dispose=count-1;
  while(t->next_dispose%3==0)--t->next_dispose;
  // Explicit worklist data is depth-dependent semantic cleanup state, not a
  // chain of eliminated tail activations. Uninitialized payloads never dispose.
  for(unsigned i=count;i>0;--i){Node *node=pending[i-1];if(node->payload){node->payload->~Payload();node->payload=nullptr;}}
  delete[] pending;delete[] nodes;
  assert(t->next_dispose==-1 && !t->last_payload && t->rendered==count && t->allocated==133333 && t->freed==t->allocated);
  assert(t->frame_max-t->frame_min<4096 && t->status->cancel.load());t->masked=false;
}
static void entry(Transfer initial) {
  auto *r=static_cast<Resume *>(initial.data);Task *t=r->task;
  arrival(t,initial);
  if(t->deep_cleanup)deep_cleanup(t);else borrowed_scope(t);
  t->loan=nullptr;t->ended=true;t->status->completed=1;
  yield(t);
  std::abort(); // Completed/abandoned contexts must never be resumed.
}
class Workers {
  std::mutex mutex;
  std::condition_variable cv;
  Task *pending[2]={nullptr,nullptr};
  bool finished[2]={false,false},busy[2]={false,false},stop=false;
  std::thread threads[2];
  void run(unsigned id) {
    assert(std::fesetround(FE_TONEAREST)==0);
    std::unique_lock<std::mutex> lock(mutex);
    for(;;) {
      cv.wait(lock,[&]{return stop || pending[id];});
      if(stop)return;
      Task *t=pending[id];pending[id]=nullptr;lock.unlock();
      Resume data{t,id};
      Transfer returned=jump_fcontext(t->context,&data);
      assert(returned.data==t);t->context=returned.context;
      lock.lock();finished[id]=true;cv.notify_all();
    }
  }
public:
  Workers() {
    threads[0]=std::thread([this]{run(0);});threads[1]=std::thread([this]{run(1);});
    assert(threads[0].get_id()!=threads[1].get_id());
  }
  void resume(Task *t,unsigned id) {
    std::unique_lock<std::mutex> lock(mutex);assert(id<2 && !busy[id] && t->context && !t->ended);
    busy[id]=true;finished[id]=false;pending[id]=t;cv.notify_all();
    cv.wait(lock,[&]{return finished[id];});busy[id]=false;
  }
  ~Workers() {
    {std::lock_guard<std::mutex> lock(mutex);stop=true;cv.notify_all();}
    for(auto &t:threads)t.join();
  }
};
struct JobState { std::mutex mutex;std::condition_variable cv;bool go=false,done=false,ready=false; };
__attribute__((noinline)) static void native_job(JobState &job,volatile std::uint64_t *loan,
                                               bool mutate,std::uintptr_t low,std::uintptr_t high) {
  alignas(4096) volatile unsigned char native_frame[256*1024];
  auto address=reinterpret_cast<std::uintptr_t>(native_frame);
  assert(address+sizeof(native_frame)<=low || address>=high);
  for(unsigned i=0;i<sizeof(native_frame);++i)native_frame[i]=static_cast<unsigned char>(i^0x5a);
  std::unique_lock<std::mutex> lock(job.mutex);job.ready=true;job.cv.notify_all();
  job.cv.wait(lock,[&]{return job.go;});
  for(unsigned i=0;i<8;++i)assert(loan[i]==pattern(i));
  if(mutate)for(unsigned i=0;i<8;++i)loan[i]=pattern(i)^UINT64_C(0xff00);
  for(unsigned i=0;i<sizeof(native_frame);++i)assert(native_frame[i]==static_cast<unsigned char>(i^0x5a));
  job.done=true;
}
static void scenario(Workers &workers,unsigned mode) {
  Monitor monitor;
  auto execution=std::make_shared<Backing>(&monitor);
  std::weak_ptr<Backing> observer=execution;
  Task task{};task.monitor=&monitor;task.mutable_loan=mode==4;task.admission_need=admission_need;
  task.low=reinterpret_cast<std::uintptr_t>(execution->low);
  task.high=reinterpret_cast<std::uintptr_t>(execution->top);
  task.context=make_fcontext(execution->top,execution->capacity,entry);
  // A routine/status handle is deliberately NOT the execution-storage owner.
  auto handle=task.status;
  auto completed_handle=handle; // Status may survive execution storage without retaining it.
  workers.resume(&task,0);assert(task.loan && task.resumes==1);
  JobState job;
  ++monitor.borrowers;
  volatile std::uint64_t *loan=task.loan;
  std::thread native([keep=execution,&monitor,&job,loan,mutate=task.mutable_loan]() mutable {
    native_job(job,loan,mutate,reinterpret_cast<std::uintptr_t>(keep->low),reinterpret_cast<std::uintptr_t>(keep->top));
    monitor.native_access_ended=++monitor.clock;
    --monitor.borrowers;keep.reset();
  });
  {std::unique_lock<std::mutex> lock(job.mutex);job.cv.wait(lock,[&]{return job.ready;});}
  if(mode==1)handle->cancel=true;
  handle.reset();assert(!observer.expired() && monitor.releases==0);
  constexpr unsigned switches=10000;
  for(unsigned i=0;i<switches;++i)workers.resume(&task,(i+1)&1);
  assert(task.workers==3 && task.resumes==switches+1);
  assert(!task.ended && task.slot_address==reinterpret_cast<std::uintptr_t>(loan));
  assert(monitor.borrowers==1 && monitor.disposers==0 && monitor.releases==0);
  if(mode==1)assert(task.masked && task.cleanup_pending);
  const bool abandon=mode==2 || mode==3 || mode==5;
  if(mode==5) {
    task.recursion_request=8;workers.resume(&task,1);
    assert(task.recursion_succeeded && task.ordinary_entered==9 && task.ordinary_completed==9);
    task.recursion_request=1000000;workers.resume(&task,0);
    assert(task.capacity_failed && task.terminated==3 && task.ordinary_entered>9 && task.ordinary_completed==9);
  }
  if(abandon) {
    if(mode==3) {
      task.terminate_requested=3;workers.resume(&task,1);
      assert(task.terminated==3 && monitor.disposers==0 && monitor.borrowers==1);
    } else if(mode==2) {task.terminated=2;task.status->completed=2;} // Entry exited; background stays parked.
    // Guest top-level exit/fatal termination: STOP execution while the owning
    // C++ scope is still suspended. Returning/unwinding would end loan lifetime.
    // No guest join/disposer is promised. Only the independent native lease remains.
    task.context=nullptr;execution.reset();
    assert(!observer.expired() && observer.use_count()==1 && monitor.releases==0);
  }
  unsigned loan_migrations=task.migrations;assert(loan_migrations>=switches);
  {std::lock_guard<std::mutex> lock(job.mutex);assert(!job.done);job.go=true;job.cv.notify_all();}
  native.join();assert(job.done && monitor.borrowers==0);
  if(!abandon) {
    task.job_joined=true;task.allow_completion=true;
    workers.resume(&task,1);
    assert(!task.ended && task.disposal_waiting && task.masked && monitor.disposers==0);
    completed_handle->cancel=true;workers.resume(&task,0);
    assert(!task.ended && task.disposal_waiting && task.masked && monitor.disposers==0);
    task.disposal_ack=true;workers.resume(&task,1);
    assert(task.ended && monitor.disposers==1 && !task.masked);
    assert(task.float_result==static_cast<double>(switches+1)*0.125);
    task.context=nullptr;execution.reset();
  } else assert(monitor.disposers==0);
  assert(observer.expired() && monitor.releases==1);
  assert(completed_handle->completed.load()==(abandon?(mode==2?2:3):1));
  assert(monitor.native_access_ended<monitor.released_at);
  if(!abandon)assert(monitor.native_access_ended<monitor.disposed_at && monitor.disposed_at<monitor.released_at);
  const char *names[]={"normal_join","cancel_masked_join","top_level_exit_retained_loan","fatal_exit_retained_loan","exclusive_native_output_join","recursive_capacity_fatal_retained_loan"};
  std::printf("{\"case\":\"%s\",\"usable_bytes\":1048576,\"worker_mask\":%u,\"switches\":%u,\"borrowed_address_stable\":true,\"loan_release_before_unmap\":true,\"backing_releases\":%u,\"disposers\":%u,\"guest_resumed_after_termination\":false,\"adjacent_canaries\":true,\"separate_native_frame_bytes\":262144,\"ordinary_entered\":%llu,\"ordinary_completed\":%llu,\"admission_need\":%llu,\"least_remaining\":%llu}\n",
              names[mode],task.workers,loan_migrations,monitor.releases.load(),monitor.disposers.load(),static_cast<unsigned long long>(task.ordinary_entered),static_cast<unsigned long long>(task.ordinary_completed),static_cast<unsigned long long>(task.admission_need),static_cast<unsigned long long>(task.least_remaining));
  if(mode==5)assert(task.admission_need>0 && task.capacity_failed && task.ordinary_completed==9);
}
static void parallel_contexts(Workers &workers) {
  Monitor monitors[3];Task tasks[3]{};std::shared_ptr<Backing> owners[3];
  std::atomic<unsigned> barrier{0};
  for(unsigned i=0;i<3;++i) {
    owners[i]=std::make_shared<Backing>(&monitors[i]);tasks[i].monitor=&monitors[i];
    tasks[i].low=reinterpret_cast<std::uintptr_t>(owners[i]->low);
    tasks[i].high=reinterpret_cast<std::uintptr_t>(owners[i]->top);
    tasks[i].context=make_fcontext(owners[i]->top,owners[i]->capacity,entry);
    tasks[i].job_joined=true;
    if(i<2)tasks[i].parallel_barrier=&barrier;
  }
  std::thread a([&]{workers.resume(&tasks[0],0);});
  std::thread b([&]{workers.resume(&tasks[1],1);});a.join();b.join();
  assert(barrier==2);workers.resume(&tasks[2],0);
  for(unsigned i=0;i<3;++i) {
    assert(!tasks[i].ended);tasks[i].allow_completion=true;tasks[i].disposal_ack=true;
    workers.resume(&tasks[i],i==1?0:1);
    assert(tasks[i].ended && tasks[i].workers==3 && monitors[i].disposers==1);
    tasks[i].context=nullptr;owners[i].reset();assert(monitors[i].releases==1);
  }
  std::puts("{\"case\":\"parallel_m_to_n_control\",\"logical_contexts\":3,\"os_workers\":2,\"parallel_entry_barrier\":2,\"all_migrated\":true,\"backing_releases\":3}");
}
static void scoped_child(Workers &workers) {
  Monitor pm,cm;Task parent{},child{};parent.monitor=&pm;child.monitor=&cm;
  auto p=std::make_shared<Backing>(&pm),c=std::make_shared<Backing>(&cm);
  parent.low=reinterpret_cast<std::uintptr_t>(p->low);parent.high=reinterpret_cast<std::uintptr_t>(p->top);
  child.low=reinterpret_cast<std::uintptr_t>(c->low);child.high=reinterpret_cast<std::uintptr_t>(c->top);
  parent.context=make_fcontext(p->top,p->capacity,entry);workers.resume(&parent,0);
  auto lease=p;child.parent_loan=parent.loan;++pm.borrowers;
  child.context=make_fcontext(c->top,c->capacity,entry);workers.resume(&child,1);
  parent.status->cancel=true;child.status->cancel=true;
  auto parent_handle=parent.status,child_handle=child.status;
  parent_handle.reset();child_handle.reset();
  for(unsigned i=0;i<1000;++i){workers.resume(&parent,(i+1)&1);workers.resume(&child,i&1);}
  assert(parent.workers==3 && child.workers==3 && pm.disposers==0 && cm.disposers==0);
  parent.allow_completion=true;workers.resume(&parent,0);
  assert(parent.join_waiting && parent.masked && !parent.ended && pm.borrowers==1);
  child.job_joined=true;child.allow_completion=true;workers.resume(&child,1);
  assert(child.disposal_waiting && child.masked && !child.ended && cm.disposers==0);
  workers.resume(&parent,1);assert(parent.join_waiting && pm.disposers==0 && pm.releases==0);
  workers.resume(&child,0);assert(child.disposal_waiting && !child.ended);
  child.disposal_ack=true;workers.resume(&child,1);
  assert(child.ended && cm.disposers==1 && pm.borrowers==1);
  // Release the parent's storage loan only after the child's actual body AND
  // suspended destructor have completed, not when cancellation was requested.
  --pm.borrowers;lease.reset();child.context=nullptr;c.reset();assert(cm.releases==1);
  parent.job_joined=true;workers.resume(&parent,0);
  assert(parent.disposal_waiting && !parent.ended && pm.disposers==0);
  parent.disposal_ack=true;workers.resume(&parent,1);assert(parent.ended && pm.disposers==1);
  parent.context=nullptr;p.reset();assert(pm.releases==1);
  std::puts("{\"case\":\"scoped_child_cleanup_before_parent_release\",\"parent_and_child_migrated\":true,\"cancel_did_not_end_join\":true,\"child_disposers\":1,\"parent_disposers\":1,\"backing_releases\":2}");
}
static void cleanup_scenario(Workers &workers,bool fail) {
  Monitor monitor;auto memory=std::make_shared<Backing>(&monitor);Task task{};
  task.monitor=&monitor;task.low=reinterpret_cast<std::uintptr_t>(memory->low);
  task.high=reinterpret_cast<std::uintptr_t>(memory->top);task.deep_cleanup=true;task.fail_worklist=fail;
  task.context=make_fcontext(memory->top,memory->capacity,entry);
  for(unsigned round=0;!task.ended && !task.terminated;++round){
    assert(round<1000);workers.resume(&task,round&1);
    if(!task.ended && !task.terminated){assert(task.masked);task.status->cancel=true;}
  }
  if(fail)assert(task.terminated==3 && !task.ended && task.allocated==0 && task.freed==0);
  else assert(task.ended && task.rendered==200000 && task.freed==133333 && task.workers==3);
  task.context=nullptr;memory.reset();assert(monitor.releases==1);
  if(fail)std::puts("{\"case\":\"native_cleanup_worklist_allocation_failure\",\"guest_terminal_escape\":true,\"recursive_fallback\":false,\"normal_resumption\":false}");
  else std::printf("{\"case\":\"native_suspending_iterative_cleanup\",\"nodes\":200000,\"initialized_payloads\":133333,\"resources_freed\":%llu,\"suspended_callbacks\":%llu,\"native_frame_span\":%llu,\"render_hash\":%llu,\"masked_cancel_pending\":true}\n",static_cast<unsigned long long>(task.freed),static_cast<unsigned long long>(task.callbacks),static_cast<unsigned long long>(task.frame_max-task.frame_min),static_cast<unsigned long long>(task.render_hash));
}
int main(int argc,char **argv) {
  assert(argc==2);admission_need=std::strtoull(argv[1],nullptr,10);
  assert(admission_need>=16384 && admission_need<1024*1024);
  Workers workers;parallel_contexts(workers);
  for(unsigned mode=0;mode<6;++mode)scenario(workers,mode);
  scoped_child(workers);cleanup_scenario(workers,false);cleanup_scenario(workers,true);
}
