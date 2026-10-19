package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Spinner
import android.widget.TextView

/** Keeps a native selection on the last published value until its owner accepts a change. */
internal class MusicSettingsChoice<T>(
    context: Context,
    private val ui: CutoutUi,
    private val label: String,
    private val choices: List<Pair<T, String>>,
    private val changed: (T, () -> Unit) -> Unit,
) : Spinner(context) {
    private var observed: T? = null
    private var allowed = true
    private var pending = false
    private var operation = 0L
    private val main = Handler(Looper.getMainLooper())

    init {
        minimumHeight = ui.dp(48)
        adapter = object : ArrayAdapter<String>(context, android.R.layout.simple_spinner_item, choices.map { it.second }) {
            init { setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item) }
            override fun getView(position: Int, convertView: View?, parent: ViewGroup): View =
                (super.getView(position, convertView, parent) as TextView).apply {
                    setTextColor(ui.foreground)
                    textSize = 17f
                    maxLines = 3
                    setPadding(ui.dp(8), ui.dp(12), ui.dp(8), ui.dp(12))
                }
        }
        onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val selected = choices.getOrNull(position)?.first ?: return
                if (!isEnabled || pending || observed == null || selected == observed || position != selectedItemPosition) return
                pending = true
                val token = ++operation
                isEnabled = false
                val completed = {
                    main.post {
                        if (token != operation) return@post
                        operation++
                        pending = false
                        isEnabled = allowed
                    }
                    Unit
                }
                try { changed(selected, completed) } catch (_: Exception) { completed() }
                observed?.let { published -> setSelection(choices.indexOfFirst { it.first == published }) }
            }
        }
    }

    fun render(value: T, enabled: Boolean = true) {
        observed = value
        allowed = enabled
        val index = choices.indexOfFirst { it.first == value }
        if (index >= 0 && selectedItemPosition != index) setSelection(index)
        isEnabled = enabled && !pending
        alpha = if (enabled) 1f else 0.55f
        contentDescription = "$label, ${choices.getOrNull(index)?.second.orEmpty()}"
    }
}
