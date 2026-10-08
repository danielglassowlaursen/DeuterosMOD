using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Deuteros.Code.Objects.ModuleTextFrame
{
	public class TextFrame
	{
		public Enums.ModuleFrameText FrameTextName { get; set; }
		public List<Line> Lines { get; set; }

		public TextFrame(Enums.ModuleFrameText frameTextName)
		{
			FrameTextName = frameTextName;
			Lines = new List<Line>();
		}
	}
}
