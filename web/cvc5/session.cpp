// Persistent SMT-LIB sessions for one dedicated worker. No process or shared-memory API.
#include <cvc5/cvc5.h>
#include <cvc5/cvc5_parser.h>
#include <emscripten/emscripten.h>
#include <emscripten/heap.h>

#include <cstdint>
#include <limits>
#include <map>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_set>

namespace {
struct Session {
    cvc5::TermManager terms;
    cvc5::Solver solver{terms};
    cvc5::parser::SymbolManager symbols{terms};
    cvc5::parser::InputParser parser{&solver, &symbols};
    std::string output;
    bool failed = false;

    explicit Session(std::uint32_t resource_limit) {
        solver.setOption("incremental", "true");
        solver.setOption("produce-models", "true");
        if (resource_limit != 0) {
            solver.setOption("rlimit-per", std::to_string(resource_limit));
        }
        parser.setIncrementalStringInput(cvc5::modes::InputLanguage::SMT_LIB_2_6, "worker-session");
    }
};

std::map<std::uint32_t, std::unique_ptr<Session>> sessions;
std::uint32_t next_id = 1;
std::string last_error;

Session& get_session(std::uint32_t id) {
    const auto entry = sessions.find(id);
    if (entry == sessions.end()) {
        throw std::invalid_argument("unknown or disposed cvc5 session");
    }
    return *entry->second;
}

void execute(Session& session, const std::string& input) {
    if (session.failed) {
        throw std::runtime_error("failed cvc5 session must be disposed");
    }
    session.output.clear();
    if (input.size() > 16 * 1024 * 1024) {
        throw std::invalid_argument("SMT command batch exceeds 16 MiB");
    }
    // Only internally generated commands enter this API, never scripts supplied in shares.
    // File I/O and process-exit commands are deliberately unavailable.
    static const std::unordered_set<std::string> allowed{
        "set-logic", "declare-fun", "declare-const", "assert", "check-sat",
        "get-value", "get-model", "get-info", "push", "pop"};
    // cvc5 1.3.4's automatic EOF reset clears its incremental-input flag.
    // Reset explicitly between complete batches. Solver and symbols stay alive.
    if (session.parser.done()) {
        session.parser.setIncrementalStringInput(cvc5::modes::InputLanguage::SMT_LIB_2_6, "worker-session");
    }
    session.parser.appendIncrementalStringInput(input + "\n");
    std::ostringstream output;
    for (;;) {
        auto command = session.parser.nextCommand();
        if (command.isNull()) {
            break;
        }
        const auto name = command.getCommandName();
        if (allowed.count(name) == 0) {
            throw std::invalid_argument("unsupported SMT command: " + name);
        }
        if (name == "check-sat") {
            // The default library ostream adds annotations such as RESOURCEOUT.
            // Keep the wire verdict explicit; get-info returns the explanation.
            const auto result = session.solver.checkSat();
            if (result.isSat()) {
                output << "sat\n";
            } else if (result.isUnsat()) {
                output << "unsat\n";
            } else if (result.isUnknown()) {
                output << "unknown\n";
            } else {
                throw std::runtime_error("cvc5 returned no check-sat verdict");
            }
        } else {
            command.invoke(&session.solver, &session.symbols, output);
        }
        // cvc5 may print a command failure instead of throwing it.
        const auto text = output.str();
        if (text.find("(error ") != std::string::npos) {
            throw std::runtime_error(text);
        }
    }
    session.output = output.str();
}
} // namespace

extern "C" {
EMSCRIPTEN_KEEPALIVE std::uint32_t sfs_session_create(std::uint32_t resource_limit) noexcept {
    last_error.clear();
    try {
        if (sessions.size() >= 32 || next_id == std::numeric_limits<std::uint32_t>::max()) {
            throw std::runtime_error("cvc5 session limit reached");
        }
        const auto id = next_id++;
        sessions.emplace(id, std::make_unique<Session>(resource_limit));
        return id;
    } catch (const std::exception& error) {
        last_error = error.what();
        return 0;
    } catch (...) {
        last_error = "unexpected exception creating cvc5 session";
        return 0;
    }
}

EMSCRIPTEN_KEEPALIVE int sfs_session_execute(std::uint32_t id, const char* input, std::uint32_t size) noexcept {
    last_error.clear();
    try {
        if (input == nullptr) {
            throw std::invalid_argument("missing SMT command batch");
        }
        if (size > 16 * 1024 * 1024) {
            throw std::invalid_argument("SMT command batch exceeds 16 MiB");
        }
        execute(get_session(id), std::string(input, size));
        return 1;
    } catch (const std::exception& error) {
        last_error = error.what();
    } catch (...) {
        last_error = "unexpected exception executing cvc5 command";
    }
    const auto entry = sessions.find(id);
    if (entry != sessions.end()) {
        entry->second->failed = true;
        entry->second->output.clear();
    }
    return 0;
}

EMSCRIPTEN_KEEPALIVE const char* sfs_session_output(std::uint32_t id) noexcept {
    const auto entry = sessions.find(id);
    return entry == sessions.end() ? "" : entry->second->output.c_str();
}

EMSCRIPTEN_KEEPALIVE const char* sfs_session_error() noexcept {
    return last_error.c_str();
}

EMSCRIPTEN_KEEPALIVE int sfs_session_destroy(std::uint32_t id) noexcept {
    return sessions.erase(id) == 1 ? 1 : 0;
}

EMSCRIPTEN_KEEPALIVE std::uint32_t sfs_session_count() noexcept {
    return static_cast<std::uint32_t>(sessions.size());
}

EMSCRIPTEN_KEEPALIVE std::size_t sfs_heap_bytes() noexcept {
    return emscripten_get_heap_size();
}
}
